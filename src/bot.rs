//! The Bluesky bot: member spotlights and weekly roundups. Credentials come
//! from the service's environment file, never from a command line.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use sqlx::PgPool;

use crate::auth::new_token;
use crate::config::Config;
use crate::models::MemberStat;
use crate::ranking;
use crate::store;

struct Session {
    token: String,
    did: String,
}

async fn rpc(
    http: &reqwest::Client,
    cfg: &Config,
    method: &str,
    body: Option<Value>,
    token: Option<&str>,
) -> Result<Value> {
    let url = format!("{}/xrpc/{}", cfg.bsky_pds, method);
    let mut req = if body.is_some() {
        http.post(&url)
    } else {
        http.get(&url)
    };
    if let Some(t) = token {
        req = req.bearer_auth(t);
    }
    if let Some(b) = body {
        req = req.json(&b);
    }
    let res = req.send().await.with_context(|| format!("{method} request"))?;
    let status = res.status();
    let text = res.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("{method} failed: {status} {text}");
    }
    Ok(serde_json::from_str(&text).unwrap_or(Value::Null))
}

async fn login(http: &reqwest::Client, cfg: &Config) -> Result<Session> {
    let handle = std::env::var("BSKY_HANDLE").context("BSKY_HANDLE is not set")?;
    let password = std::env::var("BSKY_APP_PASSWORD").context("BSKY_APP_PASSWORD is not set")?;
    let v = rpc(
        http,
        cfg,
        "com.atproto.server.createSession",
        Some(json!({ "identifier": handle, "password": password })),
        None,
    )
    .await?;
    Ok(Session {
        token: v["accessJwt"].as_str().unwrap_or_default().to_string(),
        did: v["did"].as_str().unwrap_or_default().to_string(),
    })
}

async fn post(
    http: &reqwest::Client,
    cfg: &Config,
    session: &Session,
    text: &str,
    card: Option<(&str, &str, &str)>,
) -> Result<String> {
    let mut record = json!({
        "$type": "app.bsky.feed.post",
        "text": text,
        "createdAt": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
    });
    if let Some((uri, title, description)) = card {
        record["embed"] = json!({
            "$type": "app.bsky.embed.external",
            "external": { "uri": uri, "title": title, "description": description }
        });
    }
    // The URL written in the text is plain characters to Bluesky unless it
    // carries a facet, so a click through the card is half the funnel.
    if let Some(start) = text.find("https://") {
        let url = text[start..].split_whitespace().next().unwrap_or("");
        let byte_start = text[..start].as_bytes().len();
        record["facets"] = json!([{
            "index": { "byteStart": byte_start, "byteEnd": byte_start + url.as_bytes().len() },
            "features": [{ "$type": "app.bsky.richtext.facet#link", "uri": url }]
        }]);
    }
    let v = rpc(
        http,
        cfg,
        "com.atproto.repo.createRecord",
        Some(json!({
            "repo": session.did,
            "collection": "app.bsky.feed.post",
            "record": record,
        })),
        Some(&session.token),
    )
    .await?;
    Ok(v["uri"].as_str().unwrap_or_default().to_string())
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
}

/// Three spotlights a day: one per vertical, rotating through the verticals
/// that have members, never repeating a member inside 21 days.
pub async fn spotlight(pool: &PgPool, cfg: &Config, http: &reqwest::Client, dry: bool) -> Result<()> {
    let recent = store::bot_recent_subjects(pool, "spotlight", 21).await?;
    let verticals = store::verticals(pool, false).await?;
    let day = chrono::Utc::now().timestamp() / 86_400;
    let mut posted = 0;
    for (i, v) in verticals.iter().enumerate() {
        if posted >= 3 {
            break;
        }
        // Rotate which verticals get a spotlight today.
        if ((day as usize) + i) % 3 != 0 {
            continue;
        }
        let members: Vec<MemberStat> = store::vertical_members(pool, &v.slug).await?;
        let candidate = ranking::rank_top(members)
            .into_iter()
            .find(|m| !recent.contains(&m.handle) && !m.nsfw_enabled && !m.bio.trim().is_empty());
        let Some(m) = candidate else { continue };

        let url = format!("{}/@{}", cfg.public_base, m.handle);
        let text = format!(
            "{}\n\n{} · @{} on Orbital\n{}",
            clip(&m.bio.replace('\n', " "), 150),
            v.label,
            m.handle,
            url
        );
        if dry {
            println!("--- spotlight ({}) ---\n{text}\n", v.label);
        } else {
            let session = login(http, cfg).await?;
            let uri = post(
                http,
                cfg,
                &session,
                &text,
                Some((&url, &format!("{} on Orbital", m.display_name), &clip(&m.bio, 120))),
            )
            .await?;
            store::record_bot_post(pool, "spotlight", &m.handle, Some(&uri)).await?;
        }
        posted += 1;
    }
    if posted == 0 {
        println!("no spotlight candidates today");
    }
    Ok(())
}

/// Weekly roundup: the five most-visited members in one vertical, the post that
/// people actually engage with, because a list is a thing to argue about.
pub async fn roundup(
    pool: &PgPool,
    cfg: &Config,
    http: &reqwest::Client,
    slug: Option<&str>,
    dry: bool,
) -> Result<()> {
    let verticals = store::verticals(pool, false).await?;
    let day = chrono::Utc::now().timestamp() / 86_400;
    let vertical = match slug {
        Some(s) => verticals.iter().find(|v| v.slug == s).cloned(),
        None => {
            let with_members: Vec<_> = {
                let mut out = Vec::new();
                for v in &verticals {
                    if !store::vertical_members(pool, &v.slug).await?.is_empty() {
                        out.push(v.clone());
                    }
                }
                out
            };
            if with_members.is_empty() {
                None
            } else {
                Some(with_members[(day as usize) % with_members.len()].clone())
            }
        }
    };
    let Some(v) = vertical else {
        println!("no vertical has members yet");
        return Ok(());
    };
    let members = store::vertical_members(pool, &v.slug).await?;
    let top: Vec<MemberStat> = ranking::rank_top(members)
        .into_iter()
        .filter(|m| m.visitors > 0)
        .take(5)
        .collect();
    if top.is_empty() {
        println!("{} has no measured traffic yet; skipping the roundup", v.label);
        return Ok(());
    }
    let mut text = format!("Top 5 in {} this week\n\n", v.label);
    for (i, m) in top.iter().enumerate() {
        text.push_str(&format!(
            "{}. {} (@{}) — {} visitors\n",
            i + 1,
            m.display_name,
            m.handle,
            m.visitors
        ));
    }
    let url = format!("{}/v/{}", cfg.public_base, v.slug);
    text.push_str(&format!("\n{url}"));
    let text = clip(&text, 290);
    if dry {
        println!("--- roundup ({}) ---\n{text}\n", v.label);
        return Ok(());
    }
    let session = login(http, cfg).await?;
    let uri = post(
        http,
        cfg,
        &session,
        &text,
        Some((&url, &format!("Top 5 in {}", v.label), "Ranked by distinct visitors this week.")),
    )
    .await?;
    store::record_bot_post(pool, "roundup", &v.slug, Some(&uri)).await?;
    Ok(())
}

pub async fn maintenance(pool: &PgPool) -> Result<()> {
    let promoted = store::promote_trust(pool).await?;
    let pruned = store::prune_old_events(pool).await?;
    println!("trust promoted: {promoted}, click events pruned: {pruned}");
    Ok(())
}

/// Used by `seed` to mint a password when the operator did not supply one.
pub fn random_password() -> String {
    new_token()[..24].to_string()
}
