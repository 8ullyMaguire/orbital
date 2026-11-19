//! Orbital's contribution adapter around three estate components (D31/D60).
//!
//! - `polaris-authz` owns the capability vocabulary and the verdict. Orbital only declares which
//!   role/trust may exercise each action and builds the `Subject` from its own session row.
//! - `polaris-ratelimit` owns the token bucket, the client-key rules and the honest `Retry-After`.
//! - `polaris-ledger` owns the audit-log rules, the private/public type split and the cursor.
//!
//! A fourth implementation of any of those here would be a second answer to the same question.
//! No gravity or ranking explanation enters this module (D27).

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::http::HeaderMap;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use chrono::Utc;
use polaris_authz::capability::Capability;
use polaris_authz::policy::{Grant, Policy, Requirement};
use polaris_authz::scope::Scope;
use polaris_authz::subject::Subject as AuthzSubject;
use polaris_ledger::{Actor, Redaction, Rules};
use polaris_ratelimit::{client_key, Limits, Limiter, Quota};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::error::{ApiError, AppError};
use crate::models::User;

pub const CLASS_ANON_WRITE: &str = "anon-write";
pub const CLASS_USER_WRITE: &str = "user-write";
/// A second, wider bucket. A burst allowance alone is not enough: 12/5 min is 3,456 writes a day,
/// which is exactly the "dozens of anonymous profiles a day" the launch requirement rules out.
pub const CLASS_ANON_DAY: &str = "anon-write-day";
pub const CLASS_USER_DAY: &str = "user-write-day";
pub const CLASS_READ: &str = "read";
pub const ACTION_TELEMETRY: &str = "telemetry.view";
/// Wanted links open to every signed-in user, then tighten as real contributors arrive.
pub const TELEMETRY_TL2_AT: i64 = 50;

/// Every write action that has a declared capability. A capability here with no reader in a handler
/// is a label, not a control; `tests/contribution.rs` drives the production routes for each one.
pub const ACTIONS: &[&str] = &[
    "profile.create",
    "link.add",
    "link.edit",
    "review.write",
    "ballot.cast",
    "entity.edit",
    "flag.file",
    "vertical.propose",
    "vertical.vote",
    "tag.endorse",
    "modlog.review",
    "moderation.act",
    "moderation.override",
    ACTION_TELEMETRY,
];

/// The declaration vocabulary, kept separate from `polaris-authz`'s role/requirement model so an
/// operator can still say one word per action in an env file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GateMode {
    Open,
    Login,
    Trust(i16),
    Grant,
}

impl GateMode {
    pub fn parse(key: &str, raw: &str) -> anyhow::Result<Self> {
        let raw = raw.trim();
        match raw {
            "open" => Ok(Self::Open),
            "login" => Ok(Self::Login),
            "grant" => Ok(Self::Grant),
            _ if raw.starts_with("trust(") && raw.ends_with(')') => {
                match raw[6..raw.len() - 1].parse::<i16>() {
                    Ok(v) if (0..=3).contains(&v) => Ok(Self::Trust(v)),
                    _ => Err(anyhow::anyhow!(
                        "{key} must be open|login|trust(0..3)|grant, got {raw:?}"
                    )),
                }
            }
            _ => Err(anyhow::anyhow!(
                "{key} must be open|login|trust(0..3)|grant, got {raw:?}"
            )),
        }
    }

    pub fn as_str(&self) -> String {
        match self {
            Self::Open => "open".into(),
            Self::Login => "login".into(),
            Self::Trust(n) => format!("trust({n})"),
            Self::Grant => "grant".into(),
        }
    }
}

/// Shipped posture. Contribution starts painless; reading and acting on the log is a user-driven
/// trust hierarchy. The owner's first and last say is the `admin` role granted every capability.
pub fn default_gate(action: &str) -> GateMode {
    match action {
        "link.edit" | "entity.edit" => GateMode::Login,
        "vertical.vote" | "moderation.override" => GateMode::Trust(3),
        "tag.endorse" | "modlog.review" | "moderation.act" => GateMode::Trust(2),
        ACTION_TELEMETRY => GateMode::Login,
        _ => GateMode::Open,
    }
}

/// The one place a declaration becomes an authz grant. `from_env`, `for_test` and the dynamic
/// telemetry ramp all go through it, so a mode can never mean one thing at startup and another at
/// request time.
fn grant_for(policy: &mut Policy, cap: &Capability, gate: &GateMode) {
    match gate {
        GateMode::Open => {
            policy.grant(Grant::role("everyone", cap.clone(), Scope::Site));
        }
        GateMode::Login => {
            policy.grant(Grant::role("member", cap.clone(), Scope::Site));
        }
        GateMode::Trust(n) => {
            policy.grant(
                Grant::role("member", cap.clone(), Scope::Site)
                    .requiring(Requirement::TrustAtLeast((*n).max(0) as u8)),
            );
        }
        GateMode::Grant => {} // only an explicit admin override opens this
    }
    // The instance owner always has the first and last say.
    policy.grant(Grant::role("admin", cap.clone(), Scope::Site));
}

#[derive(Clone, Debug)]
pub struct Config {
    pub policy: Policy,
    pub declaration: BTreeMap<String, String>,
    pub limits: Limits,
    /// One bucket map per instance. `Config` is held in an `Arc` by `AppState`, so cloning the
    /// config for a test shares the same budget rather than giving each handler a fresh one.
    pub limiter: Arc<Limiter>,
    pub enabled: bool,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let mut declaration = BTreeMap::new();
        let mut policy = Policy::new();
        for action in ACTIONS {
            let cap = Capability::parse(action)
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            policy.declare(cap.clone());
            let key = format!("ORBITAL_GATE_{}", action.replace('.', "_").to_uppercase());
            let default = default_gate(action).as_str();
            let raw = std::env::var(&key).unwrap_or(default);
            let gate = GateMode::parse(&key, &raw)?;
            declaration.insert(action.to_string(), gate.as_str());
            grant_for(&mut policy, &cap, &gate);
        }

        let read = Quota::per_minute(env_u32("ORBITAL_READ_BUDGET", 240)?);
        let anon_write = Quota {
            permits: env_u32("ORBITAL_ANON_WRITE_BUDGET", 12)?,
            per: Duration::from_secs(300),
        };
        let user_write = Quota {
            permits: env_u32("ORBITAL_USER_WRITE_BUDGET", 120)?,
            per: Duration::from_secs(300),
        };
        let anon_day = Quota {
            permits: env_u32("ORBITAL_ANON_DAILY_BUDGET", 8)?,
            per: Duration::from_secs(86_400),
        };
        let user_day = Quota {
            permits: env_u32("ORBITAL_USER_DAILY_BUDGET", 200)?,
            per: Duration::from_secs(86_400),
        };
        let limits = Limits::new(read)
            .class(CLASS_READ, read)
            .class(CLASS_ANON_WRITE, anon_write)
            .class(CLASS_USER_WRITE, user_write)
            .class(CLASS_ANON_DAY, anon_day)
            .class(CLASS_USER_DAY, user_day);
        limits.validate().map_err(|e| anyhow::anyhow!("{}", e))?;
        let limiter = Arc::new(Limiter::new(limits.clone()));
        Ok(Self {
            policy,
            declaration,
            limits,
            limiter,
            enabled: true,
        })
    }

    pub fn for_test() -> Self {
        let mut declaration = BTreeMap::new();
        let mut policy = Policy::new();
        for action in ACTIONS {
            let cap = Capability::parse(action).expect("declared capabilities are well formed");
            policy.declare(cap.clone());
            declaration.insert(action.to_string(), default_gate(action).as_str());
            grant_for(&mut policy, &cap, &default_gate(action));
        }
        let read = Quota::per_minute(240);
        let limits = Limits::new(read)
            .class(CLASS_READ, read)
            .class(CLASS_ANON_WRITE, Quota { permits: 12, per: Duration::from_secs(300) })
            .class(CLASS_USER_WRITE, Quota { permits: 120, per: Duration::from_secs(300) })
            .class(CLASS_ANON_DAY, Quota { permits: 8, per: Duration::from_secs(86_400) })
            .class(CLASS_USER_DAY, Quota { permits: 200, per: Duration::from_secs(86_400) });
        let limiter = Arc::new(Limiter::new(limits.clone()));
        Self {
            policy,
            declaration,
            limits,
            limiter,
            enabled: false, // a suite makes more requests than a browser
        }
    }

    /// Rebuild the policy with one action changed. Used by the tests to prove a gate really reads
    /// its declaration instead of answering from a hard-coded default.
    pub fn with_gate(mut self, action: &'static str, mode: GateMode) -> Self {
        let cap = Capability::parse(action).expect("known action");
        self.policy.grants.retain(|g| g.capability != cap);
        match mode {
            GateMode::Open => self.policy.grant(Grant::role("everyone", cap.clone(), Scope::Site)),
            GateMode::Login => self.policy.grant(Grant::role("member", cap.clone(), Scope::Site)),
            GateMode::Trust(n) => self.policy.grant(
                Grant::role("member", cap.clone(), Scope::Site)
                    .requiring(Requirement::TrustAtLeast(n.max(0) as u8)),
            ),
            GateMode::Grant => {}
        }
        self.declaration.insert(action.to_string(), mode.as_str());
        self
    }

    /// A trust-gated read or write. The estate crate returns a verdict with a reason; the route
    /// maps anonymous denials to 401 and everything else to 403.
    pub fn authorize(&self, user: Option<&User>, action: &str) -> Result<(), AppError> {
        let cap = Capability::parse(action).map_err(|e| AppError::Validation(e.to_string()))?;
        let subject = subject_for(user);
        let verdict = polaris_authz::eval::authorize(
            &self.policy,
            &subject,
            cap,
            Scope::Site,
            Utc::now().timestamp(),
        );
        if verdict.is_allowed() {
            Ok(())
        } else if user.is_none() {
            Err(AppError::Unauthorized)
        } else {
            Err(AppError::ForbiddenWithReason(verdict.reason()))
        }
    }
}

fn env_u32(key: &str, default: u32) -> anyhow::Result<u32> {
    match std::env::var(key) {
        Ok(raw) => raw
            .parse::<u32>()
            .ok()
            .filter(|n| *n > 0)
            .ok_or_else(|| anyhow::anyhow!("{key} must be a positive integer, got {raw:?}")),
        Err(_) => Ok(default),
    }
}

pub fn subject_for(user: Option<&User>) -> AuthzSubject {
    match user {
        None => AuthzSubject::new("anonymous", 0),
        Some(u) => {
            let age = Utc::now()
                .signed_duration_since(u.created_at)
                .num_days()
                .max(0) as u32;
            let mut s = AuthzSubject::new(u.id.to_string(), u.trust_level.max(0) as u8)
                .with_role("member")
                .with_account_age(age);
            if u.trust_level >= 2 {
                s = s.with_role("moderator");
            }
            if u.is_owner {
                s = s.with_role("admin");
            }
            s
        }
    }
}

/// A moderator may act on contributions by people at or below their own trust; TL3 and the owner
/// are the last say and can reverse a TL2 decision. An act against a peer is refused, so trust
/// cannot be used to settle personal scores through the queue.
pub fn can_moderate(actor: Option<&User>, target_trust: i16) -> bool {
    match actor {
        None => false,
        Some(u) if u.is_owner || u.trust_level >= 3 => true,
        Some(u) => u.trust_level >= 2 && u.trust_level > target_trust,
    }
}

// ---------------------------------------------------------------------------- rate limiting

impl Config {
    /// `Ok(())` spends a permit; `Err(secs)` is the second count to put in `Retry-After`.
    pub fn check(&self, client: &str, class: &str, now: Instant) -> Result<(), u64> {
        if !self.enabled {
            return Ok(());
        }
        let decision = self.limiter.check_at(client, class, now);
        if decision.allowed {
            Ok(())
        } else {
            Err(decision.retry_after.map_or(1, |d| d.as_secs().max(1)))
        }
    }
}

/// Resolve the declared gate before anything writes. The refusal travels with the estate's reason
/// so a 403 says what was missing instead of looking like a broken feature.
pub fn gate(st: &crate::AppState, user: Option<&User>, action: &str) -> Result<(), ApiError> {
    st.cfg.contribution.authorize(user, action).map_err(ApiError::from)
}

pub fn gate_mode<'a>(st: &'a crate::AppState, action: &str) -> &'a str {
    st.cfg.contribution.declaration.get(action).map(String::as_str).unwrap_or("open")
}

/// Spend one permit from the right class. Called after the gate and strictly before the write.
pub fn spend(st: &crate::AppState, client: &ClientKey, action: &str, user: Option<&User>) -> Result<(), ApiError> {
    let _ = action; // one write budget for all contribution actions, by client and login state
    let (day, burst) = if user.is_some() {
        (CLASS_USER_DAY, CLASS_USER_WRITE)
    } else {
        (CLASS_ANON_DAY, CLASS_ANON_WRITE)
    };
    // The daily cap is checked first so a client at its daily limit does not lose a burst permit
    // on a write that is going to be refused anyway.
    st.cfg.contribution
        .check(&client.0, day, Instant::now())
        .map_err(|secs| ApiError(AppError::RateLimited(secs)))?;
    st.cfg.contribution
        .check(&client.0, burst, Instant::now())
        .map_err(|secs| ApiError(AppError::RateLimited(secs)))
}

/// Hash the estate's chosen client address with the site salt: the bucket key and the log must not
/// carry a raw IP. `polaris-ratelimit::client_key` applies the loopback/header rule.
pub fn client_id(peer: IpAddr, headers: &HeaderMap, salt: &str) -> String {
    let cf = headers.get("cf-connecting-ip").and_then(|v| v.to_str().ok());
    let xff = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok());
    let chosen = client_key(peer, cf, xff);
    let mut hasher = Sha256::new();
    hasher.update(salt.as_bytes());
    hasher.update(b"\0");
    hasher.update(chosen.to_string().as_bytes());
    hex::encode(hasher.finalize())
}

/// An extractor so every handler gets the same client decision without a middleware that could be
/// mounted on one route and forgotten on the next.
pub struct ClientKey(pub String);

impl FromRequestParts<crate::AppState> for ClientKey {
    type Rejection = crate::error::ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &crate::AppState) -> Result<Self, Self::Rejection> {
        use std::net::{Ipv4Addr, SocketAddr};
        let peer = parts
            .extensions
            .get::<axum::extract::ConnectInfo<SocketAddr>>()
            .map(|c| c.0.ip())
            .unwrap_or_else(|| IpAddr::V4(Ipv4Addr::LOCALHOST));
        Ok(Self(client_id(peer, &parts.headers, &state.cfg.salt)))
    }
}

// ---------------------------------------------------------------------------- the log

pub struct Entry<'a> {
    pub action: &'a str,
    pub decision: &'a str,
    pub actor: Option<&'a User>,
    pub actor_handle: Option<&'a str>,
    pub visitor_hash: Option<&'a str>,
    pub subject_type: Option<&'a str>,
    pub subject_id: Option<&'a str>,
    pub before: Option<Value>,
    pub after: Option<Value>,
    pub reason: &'a str,
    pub gate: &'a str,
    pub client_hash: Option<&'a str>,
    pub request_id: Option<&'a str>,
}

pub fn rules_for(action: &str) -> Rules {
    if action.starts_with("moderation.") || action == "flag.resolve" {
        Rules::PERMANENT // a power that changes standing must say why
    } else {
        Rules::HOUSEKEEPING
    }
}

pub fn redaction_for(_action: &str) -> Redaction {
    // The log exists so TL2 can see who is doing what. A handle is already the public name on the
    // profile page; hiding it here made every contribution row anonymous and defeated review.
    // What stays private is the account id and the client hash, added only for TL3/owner.
    Redaction::PublishHandle
}

/// Build the ledger's private record from an orbital row. `project` is the only route to a public
/// shape; the store columns never go straight into an API response.
pub fn act_for<'a>(
    entry: &'a Entry<'a>,
    at: chrono::DateTime<Utc>,
) -> Result<polaris_ledger::Act, AppError> {
    let actor = match (entry.actor, entry.actor_handle) {
        (_, Some(handle)) => Actor::Person {
            account: entry.actor.map(|u| u.id.to_string()),
            public: handle.to_string(),
        },
        (Some(u), None) => Actor::Person {
            account: Some(u.id.to_string()),
            public: u.handle.clone(),
        },
        (None, None) => Actor::System,
    };
    let subject = match (entry.subject_type, entry.subject_id) {
        (Some(kind), Some(id)) => polaris_ledger::Subject::Object { kind: kind.into(), id: id.into() },
        (_, Some(id)) => polaris_ledger::Subject::Object { kind: "unknown".into(), id: id.into() },
        _ => polaris_ledger::Subject::Object { kind: "none".into(), id: String::new() },
    };
    let act = polaris_ledger::Act {
        action: entry.action.to_string(),
        subject,
        actor,
        reason: if entry.reason.is_empty() { None } else { Some(entry.reason.to_string()) },
        expires_at: None,
        at,
    };
    polaris_ledger::validate(&act, rules_for(entry.action))
        .map_err(|e| AppError::Validation(e.to_string()))?;
    Ok(act)
}

/// One row per decision, including refusals. Never called with `let _ =`: a log that failed
/// silently is worse than no log, because silence then reads as "nothing happened".
pub async fn record(pool: &PgPool, entry: Entry<'_>) -> Result<(), AppError> {
    let act = act_for(&entry, Utc::now())?;
    sqlx::query(
        "INSERT INTO contribution_log
         (action, decision, actor_id, actor_handle, visitor_hash, subject_type, subject_id,
          before_state, after_state, reason, gate_mode, client_hash, request_id)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)",
    )
    .bind(entry.action)
    .bind(entry.decision)
    .bind(entry.actor.map(|u| u.id))
    .bind(entry.actor_handle)
    .bind(entry.visitor_hash)
    .bind(entry.subject_type)
    .bind(entry.subject_id)
    .bind(entry.before)
    .bind(entry.after)
    .bind(entry.reason)
    .bind(entry.gate)
    .bind(entry.client_hash)
    .bind(entry.request_id)
    .execute(pool)
    .await
    .map(|_| {
        let _ = act; // validated before the write
    })
    .map_err(AppError::from)
}

pub async fn has_grant(pool: &PgPool, user_id: Uuid, action: &str) -> Result<bool, AppError> {
    let row: Option<(Uuid,)> = sqlx::query_as(
        "SELECT user_id FROM contribution_grants WHERE user_id = $1 AND action = $2",
    )
    .bind(user_id)
    .bind(action)
    .fetch_optional(pool)
    .await
    .map_err(AppError::from)?;
    Ok(row.is_some())
}

/// The abuse signal that decides whether a capability may be closed. Counts decisions, not rows in
/// the action tables: the refusal is the event an operator needs to see.
pub async fn summary(pool: &PgPool, days: i64) -> Result<Vec<Value>, AppError> {
    let rows = sqlx::query(
        "SELECT action, decision, count(*)::bigint AS n
         FROM contribution_log
         WHERE created_at > now() - ($1::text || ' days')::interval
         GROUP BY action, decision ORDER BY action, decision",
    )
    .bind(days.to_string())
    .fetch_all(pool)
    .await
    .map_err(AppError::from)?;
    Ok(rows
        .into_iter()
        .map(|r| serde_json::json!({
            "action": r.get::<String, _>("action"),
            "decision": r.get::<String, _>("decision"),
            "count": r.get::<i64, _>("n"),
        }))
        .collect())
}

/// The only honest "how many users" signal for tightening a gate. `count(*) FROM users` is wrong:
/// most rows here are imported shells that never signed in. Count distinct people who actually
/// contributed in the window, and split them into session-authenticated and anonymous actors.
pub async fn population_signal(pool: &PgPool, days: i64) -> Result<Value, AppError> {
    let row = sqlx::query(
        "SELECT
           count(DISTINCT actor_id) FILTER (WHERE actor_id IS NOT NULL)::bigint AS logged_in,
           count(DISTINCT actor_handle) FILTER (WHERE actor_handle IS NOT NULL)::bigint AS actors,
           count(*) FILTER (WHERE visitor_hash IS NOT NULL)::bigint AS anonymous_decisions
         FROM contribution_log
         WHERE created_at > now() - ($1::text || ' days')::interval",
    )
    .bind(days.to_string())
    .fetch_one(pool)
    .await
    .map_err(AppError::from)?;
    Ok(json!({
        "logged_in_actors": row.get::<i64, _>("logged_in"),
        "distinct_actors": row.get::<i64, _>("actors"),
        "anonymous_decisions": row.get::<i64, _>("anonymous_decisions"),
        "active_7d": active_contributors(pool, 7).await?,
        "active_30d": active_contributors(pool, 30).await?,
        "rule": "count contributors who acted, never users rows (imported shells inflate those)",
    }))
}

/// Only applied actions by a named account count as contribution activity. Refusals and anonymous
/// writes are real decisions but they are not evidence that someone is building the site.
pub async fn active_contributors(pool: &PgPool, days: i64) -> Result<i64, AppError> {
    let n: i64 = sqlx::query_scalar(
        "SELECT count(DISTINCT actor_id)::bigint FROM contribution_log
         WHERE decision = 'applied' AND actor_id IS NOT NULL
           AND created_at > now() - ($1::text || ' days')::interval",
    )
    .bind(days.to_string())
    .fetch_one(pool).await.map_err(AppError::from)?;
    Ok(n)
}

/// The gate follows activity, not a calendar: 50 contributors with an applied action in 30 days.
/// An owner's explicit declaration always wins (first and last say).
pub async fn telemetry_gate(pool: &PgPool) -> Result<GateMode, AppError> {
    let key = "ORBITAL_GATE_TELEMETRY_VIEW";
    telemetry_gate_with(pool, std::env::var(key).ok().as_deref()).await
}

/// The override is a parameter here so a test can prove it wins without mutating process-wide env
/// beside other tests. The handler is the only caller that reads the environment.
pub async fn telemetry_gate_with(pool: &PgPool, override_gate: Option<&str>) -> Result<GateMode, AppError> {
    if let Some(raw) = override_gate {
        return GateMode::parse("ORBITAL_GATE_TELEMETRY_VIEW", raw)
            .map_err(|e| AppError::Validation(e.to_string()));
    }
    Ok(if active_contributors(pool, 30).await? >= TELEMETRY_TL2_AT {
        GateMode::Trust(2)
    } else {
        GateMode::Login
    })
}

/// Build a fresh estate policy for the current ramp state and use the same verdict path as every
/// other capability. No `if trust_level >= 2` belongs in a handler.
pub async fn authorize_telemetry(st: &crate::AppState, user: &User) -> Result<GateMode, AppError> {
    let gate = telemetry_gate(&st.pool).await?;
    let cap = Capability::parse(ACTION_TELEMETRY).map_err(|e| AppError::Validation(e.to_string()))?;
    let mut policy = Policy::new();
    policy.declare(cap.clone());
    grant_for(&mut policy, &cap, &gate);
    let verdict = polaris_authz::eval::authorize(
        &policy,
        &subject_for(Some(user)),
        cap,
        Scope::Site,
        Utc::now().timestamp(),
    );
    if verdict.is_allowed() {
        Ok(gate)
    } else {
        Err(AppError::ForbiddenWithReason(verdict.reason()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn declaration_parses_and_refuses_a_typo() {
        assert_eq!(GateMode::parse("k", "open").unwrap(), GateMode::Open);
        assert_eq!(GateMode::parse("k", "trust(2)").unwrap(), GateMode::Trust(2));
        for bad in ["trust(4)", "trust(-1)", "trust", "trust(x)", "public", ""] {
            let err = GateMode::parse("ORBITAL_GATE_LINK_ADD", bad).unwrap_err().to_string();
            assert!(err.contains("ORBITAL_GATE_LINK_ADD"), "{err}");
        }
    }

    #[test]
    fn shipped_defaults_are_open_for_contribution_and_trusted_for_moderation() {
        assert_eq!(default_gate("profile.create"), GateMode::Open);
        assert_eq!(default_gate("modlog.review"), GateMode::Trust(2));
        assert_eq!(default_gate("moderation.override"), GateMode::Trust(3));
        let cfg = Config::for_test();
        assert_eq!(cfg.declaration.get("profile.create").unwrap(), "open");
        assert_eq!(cfg.declaration.get("modlog.review").unwrap(), "trust(2)");
    }

    #[test]
    fn bucket_bursts_and_keeps_classes_independent() {
        let mut tight = Config::for_test();
        tight.enabled = true;
        tight.limiter = Arc::new(Limiter::new(
            Limits::new(Quota::per_minute(240))
                .class(CLASS_ANON_WRITE, Quota { permits: 2, per: Duration::from_secs(300) })
                .class(CLASS_USER_WRITE, Quota::per_minute(120)),
        ));
        let now = Instant::now();
        assert!(tight.check("a", CLASS_ANON_WRITE, now).is_ok());
        assert!(tight.check("a", CLASS_ANON_WRITE, now).is_ok());
        assert!(tight.check("a", CLASS_ANON_WRITE, now).is_err());
        assert!(tight.check("b", CLASS_ANON_WRITE, now).is_ok());
        assert!(tight.check("a", CLASS_USER_WRITE, now).is_ok());
    }

    #[test]
    fn disabled_limiter_never_refuses() {
        let cfg = Config::for_test();
        for _ in 0..500 {
            assert!(cfg.check("a", CLASS_ANON_WRITE, Instant::now()).is_ok());
        }
    }

    #[test]
    fn client_key_prefers_cloudflare_and_never_returns_an_address() {
        let mut headers = HeaderMap::new();
        headers.insert("cf-connecting-ip", "198.51.100.7".parse().unwrap());
        headers.insert("x-forwarded-for", "203.0.113.9, 127.0.0.1".parse().unwrap());
        let peer = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let a = client_id(peer, &headers, "salt");
        assert_eq!(a.len(), 64);
        assert!(!a.contains("198.51.100"));
        headers.remove("cf-connecting-ip");
        assert_ne!(client_id(peer, &headers, "salt"), a);
        let remote = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 20));
        assert_ne!(client_id(remote, &headers, "salt"), client_id(peer, &headers, "salt"));
    }
}
