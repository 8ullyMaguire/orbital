//! Idempotent seed: the two owner accounts and their links.
//!
//! Re-running it never duplicates a link (matched on URL) and never resets a
//! password that already exists, so it is safe to run on every deploy.

use anyhow::{Context, Result};
use sqlx::PgPool;

use crate::auth::hash_password;
use crate::store;

struct SeedUser {
    handle: &'static str,
    display_name: &'static str,
    bio: &'static str,
    primary: &'static str,
    secondaries: &'static [&'static str],
    tags: &'static [&'static str],
    is_owner: bool,
    trust: i16,
    env_password: &'static str,
    links: &'static [(&'static str, &'static str, &'static str)],
}

const SFW: SeedUser = SeedUser {
    handle: "criticalnexus",
    display_name: "Critical Nexus",
    bio: "I build things and keep them running on one machine in my flat: a fanfiction archive you can keep offline, a recommender over the archive, a character game built on cards, a forge for decisions, Bluesky feeds from a hand-picked curate list, and a federated forum.\n\nNo ads, no tracking, no growth team — software I wanted to exist. All of it is open source. Start anywhere.",
    primary: "dev",
    secondaries: &["indie-web", "writers"],
    tags: &[
        "rust",
        "self-hosting",
        "fanfiction",
        "bluesky",
        "indie-web",
        "open-source",
        "atproto",
    ],
    is_owner: true,
    trust: 3,
    env_password: "SFW_ADMIN_PASSWORD",
    links: &[
        ("Everything I run", "https://blog.polarisocial.xyz/", "doc"),
        ("Leftist feeds", "https://leftist.polarisocial.xyz", "rss"),
        ("Darkhold — fanfiction archive", "https://darkhold.polarisocial.xyz", "book"),
        ("Kindred — fiction recommender", "https://kindred.polarisocial.xyz", "book"),
        ("Ascend — draw your powers", "https://ascend.polarisocial.xyz", "game"),
        ("Concord — the decision forge", "https://concord.polarisocial.xyz", "forum"),
        ("PieFed — federated forum", "https://piefed.polarisocial.xyz", "forum"),
        ("Gravity — federated social", "https://gravity.polarisocial.xyz", "globe"),
        ("The source (Forgejo)", "https://git.polarisocial.xyz", "github"),
        ("Bluesky", "https://bsky.app/profile/criticalnexus.bsky.social", "bsky"),
    ],
};

const NSFW: SeedUser = SeedUser {
    handle: "steeltitan",
    display_name: "Steel Titan",
    bio: "The adult side of what I run for the BNWO: a community site, ten Bluesky feeds from one curate list, a dating and video platform, image search, denial tracking with public boards, and the metadata server behind my Stash.\n\nEverything below is 18+.",
    primary: "nsfw",
    secondaries: &[],
    tags: &[
        "bnwo",
        "self-hosting",
        "bluesky",
        "denial",
        "community",
        "bdsm",
    ],
    is_owner: false,
    trust: 2,
    env_password: "NSFW_ADMIN_PASSWORD",
    links: &[
        ("WhiteBois — the community site", "https://whitebois.polarisocial.xyz", "forum"),
        ("BNWO feeds", "https://bnwo-feed.polarisocial.xyz", "rss"),
        ("BlackRep — dating & videos", "https://blackrep.app", "heart"),
        ("Suki — image search", "https://suki.polarisocial.xyz", "camera"),
        ("StreakForge — denial boards", "https://streakforge.polarisocial.xyz", "flame"),
        ("Stash-Box — the metadata server", "https://stashdb.polarisocial.xyz", "star"),
        ("Bluesky", "https://bsky.app/profile/steeltitan.bsky.social", "bsky"),
    ],
};

pub struct SeedReport {
    pub credentials: Vec<(String, String, String)>,
}

pub async fn seed(pool: &PgPool) -> Result<SeedReport> {
    let mut report = SeedReport {
        credentials: Vec::new(),
    };
    for spec in [&SFW, &NSFW] {
        let existing = store::user_by_handle(pool, spec.handle).await?;
        let user = match existing {
            Some(u) => u,
            None => {
                let password = match std::env::var(spec.env_password) {
                    Ok(p) if p.len() >= 8 => p,
                    _ => crate::bot::random_password(),
                };
                let hash = hash_password(&password).context("hashing the seed password")?;
                let user = store::create_user(
                    pool,
                    store::NewUser {
                        handle: spec.handle,
                        display_name: spec.display_name,
                        bio: spec.bio,
                        accent: &crate::text::accent_for(spec.handle),
                        password_hash: &hash,
                        nsfw_enabled: spec.primary == "nsfw",
                    },
                )
                .await?;
                report.credentials.push((
                    spec.handle.to_string(),
                    password,
                    if spec.is_owner { "owner".into() } else { "member".into() },
                ));
                user
            }
        };

        store::set_verticals(
            pool,
            user.id,
            spec.primary,
            &spec.secondaries.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        )
        .await?;
        store::set_tags(
            pool,
            user.id,
            &spec.tags.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        )
        .await?;
        store::set_trust(pool, user.id, spec.trust, Some(false)).await?;
        if spec.is_owner {
            sqlx::query("UPDATE users SET is_owner = true WHERE id = $1")
                .bind(user.id)
                .execute(pool)
                .await?;
        }

        // Links: matched on URL so a re-run is a no-op, and a link removed by
        // hand stays removed.
        let have = store::all_links(pool, user.id).await?;
        for (i, (label, url, icon)) in spec.links.iter().enumerate() {
            if have.iter().any(|l| l.url == *url) {
                continue;
            }
            store::create_link(pool, user.id, url, label, icon, None).await?;
            let _ = i;
        }

        // The owner's own vertical gets a curated row so the tab is not empty
        // on day one.
        store::set_curation(pool, spec.primary, &[user.id]).await?;
    }

    // Handles verified out of band stay verified: nothing here touches
    // bsky_handle.
    Ok(report)
}
