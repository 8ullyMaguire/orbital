//! The link checker. A directory of outbound links is only worth something if the links resolve,
//! and nothing else in this codebase ever asks.
//!
//! Three decisions worth stating:
//!
//! * **HEAD first, then a ranged GET.** Some hosts answer HEAD with 405/501, and treating that as
//!   death would mark live sites as broken. The GET asks for one byte and is dropped.
//! * **Private addresses are skipped, not fetched.** Link URLs come from members, so a checker is
//!   an SSRF gadget pointed at the host's own network by default: `http://127.0.0.1:8041/healthz`
//!   is a valid link. The guard is applied to the URL *and* to every redirect hop we follow.
//! * **Failure is a count, not a verdict.** `fail_count` rises on failure and resets on success;
//!   the badge appears at three. A site that is down for an hour stops advertising itself as dead
//!   the moment it answers again, with no moderator involved.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::Duration;

use anyhow::Result;
use sqlx::PgPool;
use url::Url;

use crate::store;

const TIMEOUT: Duration = Duration::from_secs(8);

/// Which HEAD answers must be re-asked with a GET before they count as a verdict.
///
/// 405/501 mean "no HEAD on this route". 400/403/404 are what HSTS-fronts and bot filters answer
/// a HEAD they never handled. 0 is the transport itself (a timeout or a TLS refusal), which a GET
/// may still survive. Everything else - a clean 200, a 5xx, a redirect - is taken at face value.
fn needs_get_confirm(status: u16) -> bool {
    status == 0 || status == 400 || status == 403 || status == 404 || status == 405 || status == 501
}
const MAX_REDIRECTS: usize = 4;
/// Three consecutive failures before the profile says anything. Two is a flaky afternoon.
pub const FAIL_THRESHOLD: i32 = 3;

/// True for addresses a link checker must never contact.
fn is_private(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                // 100.64.0.0/10 carrier NAT, and 192.0.0.0/24 special purpose.
                || v4.octets()[0] == 100 && (64..128).contains(&v4.octets()[1])
                || v4.octets()[0] == 192 && v4.octets()[1] == 0 && v4.octets()[2] == 0
                || v4 == Ipv4Addr::new(169, 254, 169, 254) // cloud metadata
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_unique_local()
                || v6.is_unicast_link_local()
                || v6.is_multicast()
                // IPv4-mapped: unwrap and re-test, or ::ffff:127.0.0.1 walks straight through.
                || v6
                    .to_ipv4_mapped()
                    .map(|v4| v4.is_loopback() || v4.is_private() || v4.is_link_local())
                    .unwrap_or(false)
                || v6 == Ipv6Addr::LOCALHOST
        }
    }
}

/// Resolve and refuse anything that points at the host or its neighbours. Resolution failure is
/// *not* proof of danger, but it is proof the link is broken, so it is reported separately.
async fn host_is_public(host: &str) -> Result<bool, String> {
    // A literal IP needs no resolver.
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(!is_private(ip));
    }
    let lower = host.to_ascii_lowercase();
    if lower == "localhost" || lower.ends_with(".localhost") || lower.ends_with(".internal") {
        return Ok(false);
    }
    let addrs = tokio::net::lookup_host((host, 0))
        .await
        .map_err(|e| e.to_string())?
        .map(|sa| sa.ip())
        .collect::<Vec<_>>();
    if addrs.is_empty() {
        return Err("no addresses".into());
    }
    Ok(addrs.iter().all(|ip| !is_private(*ip)))
}

#[derive(Debug, PartialEq)]
pub enum Checked {
    /// Answered with something that is not a client or server error.
    Ok(i32),
    /// Answered, but the answer is a failure the member should know about.
    Failed(i32),
    /// Could not be checked; not evidence the link is dead, so the counter does not move.
    Skipped(String),
}

/// One link, one verdict. `Ok` for both live and failing links - a failing link is a result, not
/// an error - and `Skipped` for anything the checker refuses to touch.
pub async fn check_one(client: &reqwest::Client, raw: &str) -> Checked {
    let Ok(url) = Url::parse(raw) else {
        return Checked::Skipped("unparseable".into());
    };
    if !matches!(url.scheme(), "http" | "https") {
        return Checked::Skipped("not http".into());
    }
    let Some(host) = url.host_str() else {
        return Checked::Skipped("no host".into());
    };
    match host_is_public(host).await {
        Ok(true) => {}
        Ok(false) => return Checked::Skipped("private address".into()),
        Err(e) => return Checked::Skipped(format!("dns: {e}")),
    }

    // Follow redirects by hand so each hop is checked: a public URL that 302s to the metadata
    // service is exactly the attack this guard exists for.
    let mut current = url;
    for _ in 0..=MAX_REDIRECTS {
        let mut resp = match client.head(current.clone()).send().await {
            Ok(r) => r,
            Err(_) => {
                // A HEAD that never completed (some servers drop it) is not a dead link; ask once
                // with a GET before believing the transport.
                match client
                    .get(current.clone())
                    .header(reqwest::header::RANGE, "bytes=0-0")
                    .send()
                    .await
                {
                    Ok(r) => r,
                    Err(_) => return Checked::Failed(0),
                }
            }
        };
        // A failing HEAD proves nothing: bluesky.app answers `HEAD /profile/...` with 404 and
        // `GET` with 200, so trusting HEAD would have marked two live profiles dead. Any status
        // that would stop the walk is confirmed with a one-byte GET first, and only the GET's
        // answer is believed.
        if needs_get_confirm(resp.status().as_u16()) {
            if let Ok(r) = client
                .get(current.clone())
                .header(reqwest::header::RANGE, "bytes=0-0")
                .send()
                .await
            {
                resp = r;
            }
        }
        let status = resp.status();
        if status.is_redirection() {
            let Some(loc) = resp
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
            else {
                return Checked::Failed(status.as_u16() as i32);
            };
            let Ok(next) = current.join(loc) else {
                return Checked::Failed(status.as_u16() as i32);
            };
            match next.host_str().map(|h| h.to_string()) {
                Some(h) => match host_is_public(&h).await {
                    Ok(true) => current = next,
                    Ok(false) => return Checked::Skipped("redirect to a private address".into()),
                    Err(e) => return Checked::Skipped(format!("redirect dns: {e}")),
                },
                None => return Checked::Failed(status.as_u16() as i32),
            }
            continue;
        }
        let code = status.as_u16() as i32;
        return if status.is_client_error() || status.is_server_error() {
            Checked::Failed(code)
        } else {
            Checked::Ok(code)
        };
    }
    Checked::Failed(0)
}

/// Walk up to `limit` links, oldest-checked first. Returns (checked, failing, skipped).
pub async fn run(pool: &PgPool, limit: i64) -> Result<(usize, usize, usize)> {
    let client = reqwest::Client::builder()
        .timeout(TIMEOUT)
        .user_agent(concat!("orbital-linkcheck/", env!("CARGO_PKG_VERSION")))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;

    let links = store::links_to_check(pool, limit).await?;
    let mut checked = 0;
    let mut failing = 0;
    let mut skipped = 0;

    for (id, url) in links {
        match check_one(&client, &url).await {
            Checked::Ok(code) => {
                store::record_link_health(pool, id, Some(code), true).await?;
                checked += 1;
            }
            Checked::Failed(code) => {
                store::record_link_health(pool, id, Some(code), false).await?;
                checked += 1;
                failing += 1;
            }
            Checked::Skipped(why) => {
                // Record the attempt, but do not punish the member for our own refusal.
                store::record_link_health(pool, id, None, true).await?;
                tracing::info!(%url, reason = %why, "link check skipped");
                skipped += 1;
            }
        }
    }
    Ok((checked, failing, skipped))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_addresses_are_refused() {
        for ip in [
            "127.0.0.1",
            "10.0.0.5",
            "192.168.1.1",
            "172.16.4.4",
            "169.254.169.254",
            "100.64.0.1",
            "::1",
            "fd00::1",
            "::ffff:127.0.0.1",
        ] {
            assert!(is_private(ip.parse().unwrap()), "{ip} must be refused");
        }
        for ip in ["1.1.1.1", "93.184.216.34", "2a00:1450:4001:80b::200e"] {
            assert!(!is_private(ip.parse().unwrap()), "{ip} must be allowed");
        }
    }

    #[test]
    fn a_suspicious_head_is_always_confirmed() {
        // Measured: bluesky.app returns 404 for HEAD and 200 for GET on the same profile URL.
        for code in [0, 400, 403, 404, 405, 501] {
            assert!(needs_get_confirm(code), "HEAD {code} must be re-asked");
        }
        for code in [200, 204, 301, 302, 500, 503] {
            assert!(!needs_get_confirm(code), "HEAD {code} is a verdict");
        }
    }

    #[test]
    fn the_failure_threshold_is_three() {
        assert_eq!(FAIL_THRESHOLD, 3);
    }
}
