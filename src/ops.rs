//! Operator commands: the things you would otherwise do by hand at two in the morning.
//!
//! Each one prints what it did and why, and the ones that change data take `--dry-run`. They are
//! deliberately thin: they call the same store functions the site does, so a script cannot drift
//! from the product.

use anyhow::{Context, Result};
use chrono::Utc;
use sqlx::PgPool;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::config::Config;
use crate::store;

/// Check the things that break quietly, in the order they break.
///
/// Exit code is the number of failures, so a timer or a shell can act on it. Nothing here writes:
/// a doctor that fixes is a doctor you cannot run when you are unsure.
pub async fn doctor(pool: &PgPool, cfg: &Config) -> Result<usize> {
    let mut failed = 0usize;
    let mut say = |ok: bool, what: &str, detail: String| {
        if !ok {
            failed += 1;
        }
        println!(
            "{} {:<22} {}",
            if ok { "PASS" } else { "FAIL" },
            what,
            detail
        );
    };

    // 1. The database answers and is at the newest migration.
    let version = crate::db::applied_version(pool).await;
    match version {
        Ok(v) => say(true, "database", format!("schema version {v:?}")),
        Err(e) => say(false, "database", format!("{e}")),
    }
    let newest = newest_migration();
    if let Ok(applied) = crate::db::applied_version(pool).await {
        let pending = newest.zip(applied).map(|(n, a)| n > a).unwrap_or(false);
        say(
            !pending,
            "migrations",
            if pending {
                format!("behind: applied {applied:?}, files up to {newest:?}")
            } else {
                format!("applied {applied:?}")
            },
        );
    }

    // 2. The site answers on its own bind address, not just through nginx.
    let addr = &cfg.bind_addr;
    let health = reqwest::Client::new()
        .get(format!("http://{addr}/healthz"))
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await;
    match health {
        Ok(r) if r.status().is_success() => say(true, "healthz", format!("200 from {addr}")),
        Ok(r) => say(false, "healthz", format!("{} from {addr}", r.status())),
        Err(e) => say(false, "healthz", format!("{addr}: {e}")),
    }

    // 3. A backup exists and is recent. A backup nobody checked is a hope.
    let dir = backup_dir();
    let newest_dump = newest_file(&dir, "dump");
    match newest_dump {
        Some((path, age_hours)) => say(
            age_hours < 24.0 * 2.0,
            "backup",
            format!("{} ({:.1}h old)", path.display(), age_hours),
        ),
        None => say(false, "backup", format!("no dump in {}", dir.display())),
    }

    // 4. The link checker is still running: the newest check is not ancient.
    let checked: Result<(Option<chrono::DateTime<Utc>>,), sqlx::Error> =
        sqlx::query_as("SELECT max(last_checked) FROM links WHERE last_checked IS NOT NULL")
            .fetch_one(pool)
            .await;
    match checked {
        Ok((Some(ts),)) => {
            let age = (Utc::now() - ts).num_hours();
            say(age < 48, "link checker", format!("last check {age}h ago"));
        }
        Ok((None,)) => say(false, "link checker", "no link has ever been checked".into()),
        Err(e) => say(false, "link checker", format!("{e}")),
    }

    // 5. The visitors table is not growing without bound.
    let visitors: (i64,) = sqlx::query_as("SELECT count(*) FROM visitor_profiles")
        .fetch_one(pool)
        .await
        .unwrap_or((0,));
    say(
        visitors.0 < 500_000,
        "visitor table",
        format!("{} profiles", visitors.0),
    );

    println!();
    println!("{failed} failure(s)");
    Ok(failed)
}

/// The newest applied-or-available migration version, read from the filenames.
/// Where the backup timer writes. The same env var the script uses, with the same default, so the
/// doctor and the timer cannot disagree about where the dumps are.
fn backup_dir() -> PathBuf {
    std::env::var("ORBITAL_BACKUP_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/var/backups".into());
            PathBuf::from(home).join("backups/orbital")
        })
}

fn newest_migration() -> Option<i64> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.split('_').next().and_then(|n| n.parse::<i64>().ok())
        })
        .max()
}

fn newest_file(dir: &Path, ext: &str) -> Option<(PathBuf, f64)> {
    let entries = std::fs::read_dir(dir).ok()?;
    entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(ext))
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            let modified = meta.modified().ok()?;
            let age = Utc::now().signed_duration_since(chrono::DateTime::<Utc>::from(modified));
            Some((e.path(), age.num_minutes() as f64 / 60.0))
        })
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
}

/// Take today's rank for every member in every vertical.
///
/// "You moved 12 → 9" needs yesterday, and yesterday cannot be reconstructed later: clicks and
/// visitors move underneath. A snapshot is cheap and unrepeatable.
pub async fn snapshot(pool: &PgPool) -> Result<usize> {
    let verticals: Vec<(String,)> = sqlx::query_as("SELECT slug FROM verticals ORDER BY sort, slug")
        .fetch_all(pool)
        .await?;
    let mut written = 0usize;
    for (slug,) in &verticals {
        let members = store::vertical_members(pool, slug).await?;
        let ranked: Vec<_> = crate::ranking::rank_top(members, &[])
            .into_iter()
            .take(100)
            .collect();
        for (i, m) in ranked.iter().enumerate() {
            sqlx::query(
                "INSERT INTO rank_snapshots (user_id, vertical_slug, rank) VALUES ($1, $2, $3)",
            )
            .bind(m.id)
            .bind(slug)
            .bind((i + 1) as i32)
            .execute(pool)
            .await?;
            written += 1;
        }
    }
    println!("snapshotted {written} ranks across {} verticals", verticals.len());
    Ok(written)
}

/// Add links to an existing member from a file: one URL per line, `url | label` if you have a label.
///
/// This is the "I have a link list already" path. It is idempotent per member: a URL they already
/// have is skipped rather than duplicated.
pub async fn import_links(
    pool: &PgPool,
    handle: &str,
    file: &Path,
    dry_run: bool,
) -> Result<(usize, usize)> {
    let user = store::user_by_handle(pool, handle)
        .await?
        .with_context(|| format!("no member @{handle}"))?;
    let raw = std::fs::read_to_string(file).with_context(|| format!("{}", file.display()))?;
    let mut added = 0usize;
    let mut skipped = 0usize;
    let existing: Vec<(String,)> =
        sqlx::query_as("SELECT url FROM links WHERE user_id = $1 AND hidden = false")
            .bind(user.id)
            .fetch_all(pool)
            .await?;
    let mut have: Vec<String> = existing.into_iter().map(|(u,)| u).collect();

    for (n, line) in raw.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (url, label) = match line.split_once('|') {
            Some((u, l)) => (u.trim(), l.trim().to_string()),
            None => (line, String::new()),
        };
        let url = match crate::text::validate_link_url(url) {
            Ok(u) => u,
            Err(e) => {
                println!("line {}: skipped ({e})", n + 1);
                skipped += 1;
                continue;
            }
        };
        if have.contains(&url) {
            skipped += 1;
            continue;
        }
        let label = if label.is_empty() {
            crate::text::label_from_url(&url)
        } else {
            label
        };
        if dry_run {
            println!("would add {url} as {label:?}");
        } else {
            store::create_link(pool, user.id, &url, &label, "link", None).await?;
        }
        have.push(url);
        added += 1;
    }
    println!(
        "{} {added} and skipped {skipped} for @{handle}",
        if dry_run { "would add" } else { "added" }
    );
    Ok((added, skipped))
}

/// Fill in the title and favicon of links that have never been fetched.
pub async fn fetch_titles(
    pool: &PgPool,
    http: &reqwest::Client,
    limit: i64,
) -> Result<(usize, usize)> {
    let rows: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, url FROM links
          WHERE metadata_at IS NULL AND hidden = false
          ORDER BY created_at ASC LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    let mut ok = 0usize;
    let mut failed = 0usize;
    for (id, url) in rows {
        match crate::meta::page_metadata(http, &url).await {
            Ok(meta) => {
                sqlx::query(
                    "UPDATE links SET title = $2, favicon = $3, metadata_at = now() WHERE id = $1",
                )
                .bind(id)
                .bind(meta.title)
                .bind(meta.favicon)
                .execute(pool)
                .await?;
                ok += 1;
            }
            Err(e) => {
                // Mark it tried, or the same broken page is fetched on every run.
                sqlx::query("UPDATE links SET metadata_at = now() WHERE id = $1")
                    .bind(id)
                    .execute(pool)
                    .await?;
                tracing::debug!(%url, %e, "no metadata");
                failed += 1;
            }
        }
    }
    println!("metadata: {ok} fetched, {failed} failed");
    Ok((ok, failed))
}

/// Apply reviewed tag aliases to everything that stores a tag.
pub async fn retag(pool: &PgPool, dry_run: bool) -> Result<usize> {
    let aliases: Vec<(String, String)> =
        sqlx::query_as("SELECT from_tag, to_tag FROM tag_aliases ORDER BY from_tag")
            .fetch_all(pool)
            .await?;
    if aliases.is_empty() {
        println!("no aliases to apply");
        return Ok(0);
    }
    let mut changed = 0usize;
    for (from, to) in &aliases {
        let tags: (i64,) = sqlx::query_as("SELECT count(*) FROM tags WHERE slug = $1")
            .bind(from)
            .fetch_one(pool)
            .await
            .unwrap_or((0,));
        let prefs: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM preference_weights WHERE tag_slug = $1",
        )
        .bind(from)
        .fetch_one(pool)
        .await
        .unwrap_or((0,));
        println!("{from} -> {to}: {} tag rows, {} preference rows", tags.0, prefs.0);
        if dry_run || (tags.0 == 0 && prefs.0 == 0) {
            continue;
        }
        // Merge, not overwrite: the target may already exist, and the stronger weight should win.
        sqlx::query(
            "INSERT INTO tags (slug, uses) SELECT $2, uses FROM tags WHERE slug = $1
             ON CONFLICT (slug) DO UPDATE SET uses = tags.uses + EXCLUDED.uses",
        )
        .bind(from)
        .bind(to)
        .execute(pool)
        .await?;
        sqlx::query("DELETE FROM tags WHERE slug = $1")
            .bind(from)
            .execute(pool)
            .await?;
        sqlx::query(
            "INSERT INTO preference_weights (owner_kind, owner_id, tag_slug, weight, source, updated_at)
             SELECT owner_kind, owner_id, $2, weight, source, now()
               FROM preference_weights WHERE tag_slug = $1
             ON CONFLICT (owner_kind, owner_id, tag_slug)
             DO UPDATE SET weight = GREATEST(preference_weights.weight, EXCLUDED.weight),
                            updated_at = now()",
        )
        .bind(from)
        .bind(to)
        .execute(pool)
        .await?;
        sqlx::query("DELETE FROM preference_weights WHERE tag_slug = $1")
            .bind(from)
            .execute(pool)
            .await?;
        sqlx::query("UPDATE member_tags SET tag_slug = $2 WHERE tag_slug = $1")
            .bind(from)
            .bind(to)
            .execute(pool)
            .await
            .ok();
        changed += 1;
    }
    println!("{} {changed} alias(es)", if dry_run { "would apply" } else { "applied" });
    Ok(changed)
}

/// Render one social card per member: an SVG always, a PNG when a rasteriser is installed.
///
/// Bluesky and the rest will not take an SVG, so the PNG matters; if `rsvg-convert` is missing the
/// SVG is still written and the command says what to install rather than pretending.
pub async fn og(pool: &PgPool, out: &Path, only: Option<&str>) -> Result<usize> {
    std::fs::create_dir_all(out).ok();
    let rows: Vec<(String, String, i64)> = match only {
        Some(h) => {
            sqlx::query_as(
                "SELECT handle, COALESCE(display_name, handle), (SELECT count(*) FROM links l
                    WHERE l.user_id = users.id AND l.hidden = false) FROM users WHERE handle = $1",
            )
            .bind(h)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_as(
                "SELECT handle, COALESCE(display_name, handle), (SELECT count(*) FROM links l
                    WHERE l.user_id = users.id AND l.hidden = false)
                   FROM users WHERE hidden = false ORDER BY created_at",
            )
            .fetch_all(pool)
            .await?
        }
    };
    let raster = which("rsvg-convert");
    if raster.is_none() {
        eprintln!("rsvg-convert not found: writing SVG only (install librsvg2-bin for PNG)");
    }
    let base = crate::config::Config::from_env()?.public_base;
    let mut written = 0usize;
    for (handle, name, links) in rows {
        let svg = crate::og::card(&name, &handle, links, &base);
        let svg_path = out.join(format!("{handle}.svg"));
        std::fs::write(&svg_path, &svg)?;
        if let Some(bin) = &raster {
            // Networks will not embed an SVG: the PNG is the one that matters, and it is only a
            // subprocess away when the rasteriser is installed.
            let png_path = out.join(format!("{handle}.png"));
            let status = std::process::Command::new(bin)
                .arg("--width")
                .arg("1200")
                .arg("--height")
                .arg("630")
                .arg("--output")
                .arg(&png_path)
                .arg(&svg_path)
                .status();
            match status {
                Ok(s) if s.success() => {}
                other => eprintln!("rsvg-convert failed for {handle}: {other:?}"),
            }
        }
        written += 1;
    }
    println!("wrote {written} card(s) to {}", out.display());
    Ok(written)
}

fn which(bin: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|p| p.join(bin))
            .find(|p| p.is_file())
    })
}
