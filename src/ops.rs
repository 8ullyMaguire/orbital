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

    // 1b. Nothing a test wrote is sitting in the public catalogue. A fixture is invisible (hidden) and therefore
    // easy to forget, which is exactly why it is a FAIL and not a note: it is still counted in a vertical.
    match crate::store::fixtures(pool).await {
        Ok(f) if f.is_empty() => say(true, "fixtures", "none".into()),
        Ok(f) => say(
            false,
            "fixtures",
            format!(
                "{} hidden fixture member(s), {} link(s) — run `orbital cleanup --apply`",
                f.len(),
                f.iter().map(|x| x.links).sum::<i64>()
            ),
        ),
        Err(e) => say(false, "fixtures", format!("{e}")),
    }

    // 1c. The entity substrate must not lag the roster. A backfill that never ran leaves a
    // catalogue-shaped site with no catalogue, which is invisible from the public pages.
    match crate::store::catalogue_composition(pool).await {
        Ok(c) => say(
            c.shells <= c.subjects,
            "catalogue",
            format!(
                "{} shell(s), {} subject(s), {} entities, {} receipts",
                c.shells, c.subjects, c.entities, c.receipts
            ),
        ),
        Err(e) => say(false, "catalogue", format!("{e}")),
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

    // 4b. The failure count is meaningless without its denominator: 267k links and one failing is
    // a different site from 20 links and one failing. Print both; do not turn coverage into a gate.
    match crate::store::link_health_coverage(pool).await {
        Ok(c) => say(true, "link coverage", c.to_string()),
        Err(e) => say(false, "link coverage", format!("{e}")),
    }

    // 5. The uptime probe is still stamping, and what it stamped was a 200. The probe writes a
    // file; nothing else here would notice a host that stopped answering.
    let stamp = std::fs::read_to_string(backup_dir().join("uptime.txt")).unwrap_or_default();
    match stamp.lines().last() {
        Some(line) => {
            let mut parts = line.split_whitespace();
            let at = parts.next().unwrap_or("");
            let code = parts.next().unwrap_or("");
            let parsed = chrono::DateTime::parse_from_rfc3339(at).ok();
            let fresh = parsed
                .map(|t| (Utc::now() - t.with_timezone(&Utc)).num_minutes() < 30)
                .unwrap_or(false);
            say(
                fresh && code == "200",
                "uptime probe",
                format!("{code} at {at}{}", if fresh { "" } else { " (stale)" }),
            );
        }
        None => say(false, "uptime probe", "no stamp; the timer has never run".into()),
    }

    // 6. The visitors table is not growing without bound.
    let visitors: (i64,) = sqlx::query_as("SELECT count(*) FROM visitor_profiles")
        .fetch_one(pool)
        .await
        .unwrap_or((0,));
    say(
        visitors.0 < 500_000,
        "visitor table",
        format!("{} profiles", visitors.0),
    );

    // 7. The instance's taste, folded from the sources this instance declares (D30). This is the
    //    report's only home: an absent source is a status, not a failure, because a new site declares
    //    accounts before those accounts have left signals.
    match crate::taste::load(pool, &cfg.taste.declaration).await {
        Ok(folded) => {
            for (what, detail) in folded.report_lines() {
                say(true, &what, detail);
            }
            say(true, "taste profile", folded.summary());
        }
        Err(e) => say(false, "taste profile", format!("{e}")),
    }

    // 8. What each surface declares, so an operator can see the file's effect without reading rankings.
    //    Four lines, one per surface, and no surface's number appears on another's line.
    for surface in crate::dial::Surface::ALL {
        let policy = cfg.dial.policy(surface);
        say(
            true,
            &format!("dial/{}", surface.name()),
            format!(
                "weight {:.2}, ordered {:.2}, ceiling {:.2}{}",
                policy.weight,
                policy.ordered,
                policy.ceiling.strength,
                if policy.ceiling.locked { " (locked)" } else { "" }
            ),
        );
    }

    // 9. What a click is worth (D41), stated as the ratio it promises rather than the constant it uses.
    say(true, "attention", cfg.attention.sentence());

    // 10. Fame, in both windows it is measured in (D42): the leader `Top` would show today, and what the
    // monument keeps. Printed together because a declaration that says "90 days" beside a page that shows a
    // lifetime number is the exact mistake this step exists to prevent.
    let everyone = store::all_members(pool, true).await?;
    if let Some(lead) = everyone
        .iter()
        .max_by(|a, b| a.recent.partial_cmp(&b.recent).unwrap_or(std::cmp::Ordering::Equal))
    {
        say(
            true,
            "fame",
            format!(
                "Top reads a 90-day half-life; the monument keeps the lifetime total — leader @{} {:.1} weighted over 90 days / {} clicks all time",
                lead.handle,
                lead.recent,
                crate::render::fmt_exact(lead.clicks)
            ),
        );
    } else {
        say(true, "fame", "Top reads a 90-day half-life; no members yet".to_string());
    }

    // 11. How many slots one member may hold, and how much of a shelf is the tail (D43).
    say(true, "exposure", cfg.exposure.sentence());

    // 12. The instance's declared floors (D51). "Declared" and "declared and applied" are different facts, and
    // only the second changes a page — so each declared name is printed with what it matched.
    let every_member = store::all_members(pool, true).await?;
    let notes = cfg.floors.report(&every_member);
    if notes.is_empty() {
        say(true, "floors", "none declared (FLOORS_FILE absent)".to_string());
    } else {
        for n in &notes {
            say(
                n.matched > 0,
                "floors",
                format!("{} {} · {}", n.name, n.weight, n.sentence(&every_member)),
            );
        }
    }

    // 12. What the ceiling sees, and where the tail will point today (D45). Printed because a rule about
    // exposure that nobody can read is a rule nobody can check.
    let window = store::impressions_window(pool, "home", 24).await?;
    let total: i64 = window.values().sum();
    let capped = window
        .iter()
        .filter(|(_, n)| crate::ceiling::over_ceiling(**n, total, &cfg.ceiling))
        .count();
    say(
        true,
        "ceiling",
        format!(
            "{} — home has {} impression(s) in 24h from {} member(s), {} over their share",
            cfg.ceiling.sentence(),
            total,
            window.len(),
            capped
        ),
    );

    // 13. Who the undiscovered shelf would consider (D46): the cut, and how many members sit below it. On a
    // site smaller than its own page the answer is "everybody is already on it", and the shelf is absent —
    // printed here so the absence is a measurement rather than a mystery.
    let everyone = store::all_members(pool, true).await?;
    let exposure: Vec<f64> = everyone.iter().map(|m| m.recent).collect();
    let cut = crate::exposure::percentile(&exposure, crate::shelf::UNDISCOVERED_PERCENTILE);
    let below = exposure.iter().filter(|e| **e < cut).count();
    say(
        true,
        "undiscovered",
        format!(
            "cut {:.1} of exposure ({:.0}th percentile), {} of {} member(s) below it",
            cut,
            crate::shelf::UNDISCOVERED_PERCENTILE * 100.0,
            below,
            everyone.len()
        ),
    );

    // 14. How concentrated each surface's attention is (D34, SPEC §2c). The number the fame headwind is
    //    judged by, printed here and nowhere else: a public concentration figure says whose work the site
    //    favours, which is not a sentence a page may carry.
    match crate::concentration::latest(pool).await {
        Ok(rows) if rows.is_empty() => say(
            true,
            "concentration",
            "no snapshot yet; run `orbital snapshot` after the week's clicks".into(),
        ),
        Ok(rows) => {
            for row in &rows {
                println!("     {:<22} {}", "", row.sentence());
            }
            say(true, "concentration", format!("{} surface(s) measured", rows.len()));
        }
        Err(e) => say(false, "concentration", format!("{e}")),
    }

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
pub async fn snapshot(pool: &PgPool, as_of: chrono::DateTime<Utc>) -> Result<usize> {
    let verticals: Vec<(String,)> = sqlx::query_as("SELECT slug FROM verticals ORDER BY sort, slug")
        .fetch_all(pool)
        .await?;
    let mut written = 0usize;
    for (slug,) in &verticals {
        let members = store::vertical_members(pool, slug).await?;
        let ranked: Vec<_> = crate::ranking::rank_top(members, &[], 1.0, crate::attention::Config::raw(), crate::ranking::Window::Lifetime)
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
    // The same week's concentration, keyed by the ISO week so a re-run corrects the row.
    for row in crate::concentration::record(pool, as_of).await? {
        println!("  {}", row.sentence());
    }
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
    let mut rasterised = 0usize;
    for (handle, name, links) in rows {
        let svg = crate::og::card(&name, &handle, links, &base);
        let svg_path = out.join(format!("{handle}.svg"));
        let png_path = out.join(format!("{handle}.png"));
        // This runs on every deploy, on the daily metadata timer and in the unit's `ExecStartPost`,
        // and each card is an `rsvg-convert` process. Once the roster is four digits that is nineteen
        // minutes per restart, nearly all of it re-drawing pictures that did not change — so a card is
        // only written when its text differs from what is on disk.
        let same_svg = !stale(&svg_path, &svg);
        if !same_svg {
            std::fs::write(&svg_path, &svg)?;
        }
        if let Some(bin) = &raster {
            // Networks will not embed an SVG: the PNG is the one that matters, and it is only a
            // subprocess away when the rasteriser is installed. A missing PNG is always redrawn, so
            // installing the rasteriser later still fills the directory.
            if !same_svg || !png_path.exists() {
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
                    Ok(s) if s.success() => rasterised += 1,
                    other => eprintln!("rsvg-convert failed for {handle}: {other:?}"),
                }
            }
        }
        written += 1;
    }
    println!(
        "wrote {written} card(s) to {} ({rasterised} rasterised)",
        out.display()
    );
    Ok(written)
}

/// Is the file at `path` different from `text`? A missing file is stale; an unreadable one is stale.
/// Split out from `og` so the decision is testable without a database or a rasteriser.
fn stale(path: &Path, text: &str) -> bool {
    std::fs::read_to_string(path).map(|old| old != text).unwrap_or(true)
}

#[cfg(test)]
mod stale_tests {
    use super::stale;

    #[test]
    fn a_missing_or_changed_card_is_stale_and_an_identical_one_is_not() {
        let dir = std::env::temp_dir().join(format!("orbital-og-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("card.svg");
        let _ = std::fs::remove_file(&path);

        assert!(stale(&path, "one"), "a missing card must be written");
        std::fs::write(&path, "one").unwrap();
        assert!(!stale(&path, "one"), "an identical card must be skipped");
        assert!(stale(&path, "two"), "a changed card must be written");
        std::fs::remove_dir_all(&dir).ok();
    }
}

fn which(bin: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|p| p.join(bin))
            .find(|p| p.is_file())
    })
}


fn say_line(line: &str) {
    println!("  {line}");
}

/// `orbital impressions [--date YYYY-MM-DD] [--days N]`: what the ceiling sees right now, and the tail the rotation
/// will fill on that date and the next — printed from the date alone, so two hosts can compare notes.
/// The freeze's housekeeping: a smoke test wrote members and links into the live database on 2026-10-29 and
/// nothing since has noticed them. This reports what it found and, with `--apply`, removes exactly that.
pub async fn cleanup(pool: &PgPool, apply: bool) -> Result<()> {
    let found = crate::store::fixtures(pool).await?;
    if found.is_empty() {
        println!("fixtures: none");
        return Ok(());
    }
    let links: i64 = found.iter().map(|f| f.links).sum();
    for f in &found {
        println!("fixture  @{}  ({} link(s), hidden)", f.handle, f.links);
    }
    if !apply {
        println!("{} member(s), {} link(s) — dry run; pass --apply to delete", found.len(), links);
        return Ok(());
    }
    let (users, links) = crate::store::delete_fixtures(pool).await?;
    println!("deleted {} member(s) and {} link(s)", users, links);
    Ok(())
}

pub async fn impressions(
    pool: &PgPool,
    cfg: &Config,
    date: Option<String>,
    days: Option<i64>,
) -> Result<()> {
    let hours: i64 = days.unwrap_or(1).max(1) * 24;
    let day = match date {
        Some(raw) => chrono::NaiveDate::parse_from_str(&raw, "%Y-%m-%d")
            .with_context(|| format!("`{raw}` is not a date (YYYY-MM-DD)"))?,
        None => chrono::Utc::now().date_naive(),
    };

    for surface in ["home"] {
        let window = store::impressions_window(pool, surface, hours as i32).await?;
        let total: i64 = window.values().sum();
        println!(
            "{surface}: {total} impression(s) in the last {hours}h, {} member(s)",
            window.len()
        );
        let members = store::all_members(pool, true).await?;
        for m in &members {
            let mine = window.get(&m.id).copied().unwrap_or(0);
            if mine == 0 {
                continue;
            }
            let share = if total > 0 { mine as f64 / total as f64 } else { 0.0 };
            println!(
                "  @{:<20} {:>5} ({:.0}%){}",
                m.handle,
                mine,
                share * 100.0,
                if crate::ceiling::over_ceiling(mine, total, &cfg.ceiling) {
                    "  CAPPED"
                } else {
                    ""
                }
            );
        }

        // The long-tail share (D50): how much of the surface went to the members below the median exposure.
        // Printed with the floor attached, because the number is worthless and misleading under it.
        if days.is_some() {
            let rows: Vec<(f64, i64)> = members
                .iter()
                .map(|m| (m.recent, window.get(&m.id).copied().unwrap_or(0)))
                .collect();
            match crate::concentration::long_tail(&rows) {
                Some(mut lt) => {
                    lt.window_hours = hours;
                    say_line(&lt.verdict());
                }
                None => say_line("no impressions in the window, so there is no share to print"),
            }
        }

        let ids: Vec<uuid::Uuid> = members.iter().map(|m| m.id).collect();
        let order: Vec<usize> = (0..members.len()).collect();
        for offset in 0..2 {
            let d = day + chrono::Duration::days(offset);
            let tail = crate::ceiling::rotate(
                &order,
                &order,
                &ids,
                crate::ceiling::day_key(d),
                &cfg.ceiling,
            );
            let names: Vec<String> = tail
                .iter()
                .take(cfg.ceiling.tail_slots.max(1).min(tail.len()))
                .map(|i| format!("@{}", members[*i].handle))
                .collect();
            println!("  tail for {d}: {}", if names.is_empty() { "(nobody yet)".to_string() } else { names.join(" ") });
        }
    }

    Ok(())
}

/// Fill the catalogue and leave the receipt the four earlier imports never wrote.
pub async fn backfill_catalogue(pool: &PgPool, limit: Option<i64>, dry_run: bool) -> Result<()> {
    if dry_run {
        let c = crate::store::catalogue_composition(pool).await?;
        println!(
            "dry run: {} shell(s) without a catalogue row, {} entities, {} categories today",
            c.shells.saturating_sub(c.subjects),
            c.entities,
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM categories")
                .fetch_one(pool)
                .await?
        );
        return Ok(());
    }
    let c = crate::store::backfill_catalogue(pool, limit).await?;
    println!(
        "catalogue: seen {} inserted {} conflicts {} tags {}",
        c.seen, c.inserted, c.conflicts, c.tags
    );
    sqlx::query(
        "INSERT INTO import_runs (source, rows_seen, rows_inserted, rows_failed, note, finished_at)
         VALUES ('catalogue-backfill', $1, $2, $3, $4, now())",
    )
    .bind(c.seen)
    .bind(c.inserted)
    .bind(c.conflicts)
    .bind(format!("entity-first refactor M1; limit={limit:?}"))
    .execute(pool)
    .await?;
    Ok(())
}

/// Erase one imported shell and everything that names it, in one transaction. An entity shared by
/// another subject is kept: erasing one of several people behind a canonical surface must not erase
/// the others. A second run reports all zeros, which is the idempotency gate.
pub async fn erase_shell(pool: &PgPool, handle: &str, apply: bool) -> Result<crate::models::EraseReport> {
    let mut tx = pool.begin().await?;
    let user: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM users WHERE handle = $1 FOR UPDATE")
            .bind(handle)
            .fetch_optional(&mut *tx)
            .await?;
    let Some((user_id,)) = user else {
        return Ok(crate::models::EraseReport {
            user: 0,
            entities: 0,
            links: 0,
            tags: 0,
            visits: 0,
        });
    };
    let entities: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM entities e
          WHERE (slug = $1 OR EXISTS (
                    SELECT 1 FROM entity_people ep WHERE ep.entity_id = e.id AND ep.user_id = $2))
            AND NOT EXISTS (
                    SELECT 1 FROM entity_people other
                     WHERE other.entity_id = e.id AND other.user_id <> $2)",
    )
    .bind(handle)
    .bind(user_id)
    .fetch_all(&mut *tx)
    .await?;

    let links: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM links WHERE user_id = $1 OR entity_id = ANY($2)",
    )
    .bind(user_id)
    .bind(&entities)
    .fetch_one(&mut *tx)
    .await?;
    let tags: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM tags WHERE user_id = $1)
              + (SELECT count(*) FROM entity_tags WHERE entity_id = ANY($2))::bigint",
    )
    .bind(user_id)
    .bind(&entities)
    .fetch_one(&mut *tx)
    .await?;
    let visits: i64 =
        sqlx::query_scalar("SELECT count(*)::bigint FROM source_hits WHERE user_id = $1")
            .bind(user_id)
            .fetch_one(&mut *tx)
            .await?;
    let report = crate::models::EraseReport {
        user: 1,
        entities: entities.len() as i64,
        links,
        tags,
        visits,
    };

    if apply {
        sqlx::query("DELETE FROM links WHERE user_id = $1 OR entity_id = ANY($2)")
            .bind(user_id)
            .bind(&entities)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM tags WHERE user_id = $1")
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM entities WHERE id = ANY($1)")
            .bind(&entities)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO import_runs (source, rows_seen, rows_inserted, note, finished_at)
             VALUES ('erasure', $1, $2, $3, now())",
        )
        .bind(report.entities + links + tags + visits + 1)
        .bind(report.user + report.entities + links + tags + visits)
        .bind(format!("entity-first refactor M3; handle={handle}"))
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(report)
}

/// A bounded snapshot of claim conversion over a fixed window. No writes: this is the number M3b
/// fixed before M4 may import more rows, so it has to be reproducible by an operator command.
pub async fn claim_conversion(
    pool: &PgPool,
    days: i64,
) -> Result<crate::models::ClaimConversion> {
    let receipts: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM import_runs
          WHERE source = 'claim-proof' AND started_at > now() - make_interval(days => $1::int)",
    )
    .bind(days)
    .fetch_one(pool)
    .await?;
    let claims: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM users
          WHERE claimed_at IS NOT NULL AND claimed_at > now() - make_interval(days => $1::int)",
    )
    .bind(days)
    .fetch_one(pool)
    .await?;
    let shells: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM users
          WHERE claimed_at IS NULL AND password_hash IS NULL",
    )
    .fetch_one(pool)
    .await?;
    let erasures: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM import_runs
          WHERE source = 'erasure' AND started_at > now() - make_interval(days => $1::int)",
    )
    .bind(days)
    .fetch_one(pool)
    .await?;
    Ok(crate::models::ClaimConversion {
        days,
        receipts,
        claims,
        shells,
        erasures,
    })
}

/// Read the append-only decision log. Deliberately read-only: a CLI that could edit history would
/// make the history a claim rather than a record. The HTTP surface applies the same filters with
/// typed binds; this command is the operator's offline view of the same rows.
pub async fn modlog(
    pool: &PgPool,
    action: Option<&str>,
    decision: Option<&str>,
    q: Option<&str>,
    limit: i64,
) -> Result<()> {
    use sqlx::{QueryBuilder, Row};
    let mut builder = QueryBuilder::new(
        "SELECT created_at, action, decision, actor_handle, subject_type, subject_id, reason, gate_mode
         FROM contribution_log WHERE true",
    );
    if let Some(v) = action {
        builder.push(" AND action = ").push_bind(v.to_string());
    }
    if let Some(v) = decision {
        builder.push(" AND decision = ").push_bind(v.to_string());
    }
    if let Some(v) = q {
        let like = format!("%{}%", v.replace('%', "\\%").replace('_', "\\_"));
        builder
            .push(" AND (reason ILIKE ")
            .push_bind(like.clone())
            .push(" ESCAPE '\\' OR subject_id ILIKE ")
            .push_bind(like)
            .push(" ESCAPE '\\')");
    }
    builder
        .push(" ORDER BY created_at DESC, id DESC LIMIT ")
        .push_bind(limit.clamp(1, 200));
    let rows = builder.build().fetch_all(pool).await?;
    println!("ts | action | decision | actor | subject | gate | reason");
    for row in &rows {
        let at: chrono::DateTime<Utc> = row.try_get("created_at")?;
        println!(
            "{} | {} | {} | {} | {}/{} | {} | {}",
            at.to_rfc3339(),
            row.try_get::<String, _>("action")?,
            row.try_get::<String, _>("decision")?,
            row.try_get::<Option<String>, _>("actor_handle")?.unwrap_or_else(|| "-".into()),
            row.try_get::<Option<String>, _>("subject_type")?.unwrap_or_else(|| "-".into()),
            row.try_get::<Option<String>, _>("subject_id")?.unwrap_or_else(|| "-".into()),
            row.try_get::<String, _>("gate_mode")?,
            row.try_get::<Option<String>, _>("reason")?.unwrap_or_default(),
        );
    }
    println!("{} row(s)", rows.len());
    Ok(())
}
