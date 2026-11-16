# PLAN v4 — the entity-first refactor

Implements `docs/SPEC.md` §7 (D58). The repo's plan of record until a step is closed. Every step
names exact files, complete code and the command whose real output closes it. Steps M2–M6 are an
ordering contract: their detailed sections get written immediately before they start, so this file
does not contain imagined code for work nobody has begun.

Live baseline measured 2026-11-16: 48,153 users / 48,151 shells / 0 entities / 0 categories /
0 import runs; `/` 3.1–3.8 s, `/v/nsfw` 4.8 s.

## Order, and why

M0 makes the ledger honest: without it the next session takes a Nov 10 step against a Nov 16 site.
M1 fills the substrate that M2 needs — you cannot build an entity read path over an empty table and
call it a migration. M2 moves the read path one surface at a time (home first, then verticals,
search and feeds) so each surface has its own gate and the roster never silently disappears. M3 adds
claim proof and erasure before any further bulk import, because a claim on 48k people without a
takedown path is a publication, not a directory. M4 receipted ingestion makes future imports
idempotent by construction. M5 is the scale contract and link-health budget. M6 puts the eval and
golden set before any ranking change on the new catalogue.

---

## M0 — the ledger is the live site (docs only)

Files: `docs/ROADMAP.md` (header + §4 open list), `docs/DECISIONS.md` (D58),
`~/secondbrain/10-Projects/orbital-handoff.md`, `~/secondbrain/10-Projects/orbital-goal-prompt.md`.

Change: replace the "2 members / 211 tests" header with the measured live numbers; replace the
13-step Nov 10 list with M1–M6 above, each carrying its own gate; keep `Parked` and `Done` intact as
history; record D58 with the rejected alternatives (keep member-centric, hide all shells, new
`stubs` table); rewrite the handoff's numbers and next-step block.

Verify:

    head -8 docs/ROADMAP.md
    # expects 48,153 members, 0 entities, 0 receipts, and the date 2026-11-16

    grep -n '^## D58' docs/DECISIONS.md
    # expects one line with D58

    curl -s https://orbital.polarisocial.xyz/api/v1/stats
    # expects {"clicks":...,"links":{...},"members":48153,"verticals":10,"visitors":...}

---

## M1 — fill the entity substrate from imported shells (this turn's code step)

No migration: `entities`, `categories`, `entity_tags`, `entity_people` and `import_runs` already
exist from `0009_entities.sql` and are empty. Filling them is adoption, not construction.

### 1. `src/models.rs` — two new public shapes

Append:

```rust
/// What the catalogue looks like right now. `shells` counts imported rows with no way to log in;
/// `subjects` counts shells that already have a catalogue row. The doctor gate is `shells <= subjects`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CatalogueComposition {
    pub shells: i64,
    pub claimed: i64,
    pub entities: i64,
    pub subjects: i64,
    pub receipts: i64,
}

/// The four numbers one backfill run is accountable for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BackfillCounts {
    pub seen: i64,
    pub inserted: i64,
    pub conflicts: i64,
    pub tags: i64,
}
```

### 2. `src/store.rs` — instrument and backfill

Append:

```rust
/// One read for the doctor and the operator. Every count is cast: `count(*)` is int4.
pub async fn catalogue_composition(pool: &PgPool) -> Result<CatalogueComposition> {
    let row = sqlx::query_as::<_, CatalogueComposition>(
        "SELECT
           (SELECT count(*)::bigint FROM users
             WHERE claimed_at IS NULL AND password_hash IS NULL AND hidden = false) AS shells,
           (SELECT count(*)::bigint FROM users
             WHERE claimed_at IS NOT NULL OR password_hash IS NOT NULL) AS claimed,
           (SELECT count(*)::bigint FROM entities WHERE hidden = false) AS entities,
           (SELECT count(*)::bigint FROM entity_people WHERE role = 'subject') AS subjects,
           (SELECT count(*)::bigint FROM import_runs) AS receipts",
    )
    .fetch_one(pool)
    .await?;
    Ok(row)
}

/// Slugs are human-readable but uniqueness is not left to chance: an eight-hex suffix makes two
/// handles that normalise to the same slug different rows. A claim can give the entity a clean slug.
fn entity_slug(handle: &str) -> String {
    let raw: String = handle
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let trimmed = raw.trim_matches('-');
    let base = if trimmed.len() < 2 {
        format!("m-{trimmed}")
    } else {
        trimmed.to_string()
    };
    let base: String = base.chars().take(51).collect();
    format!("{}-{}", base.trim_matches('-'), &Uuid::new_v4().simple().to_string()[..8])
}

/// Fill the substrate from shells that do not have a catalogue row yet. A shell is a user with no
/// password and no claim; an account is never backfilled as someone else's subject. Tags that do not
/// satisfy `entity_tags`' own CHECK are counted out here instead of aborting the whole run.
pub async fn backfill_catalogue(pool: &PgPool, limit: Option<i64>) -> Result<BackfillCounts> {
    sqlx::query(
        "INSERT INTO categories (slug, label, description, sort, nsfw)
         SELECT slug, label, description, sort, nsfw FROM verticals
         ON CONFLICT (slug) DO NOTHING",
    )
    .execute(pool)
    .await?;

    let rows: Vec<(Uuid, String, String, Option<String>, bool, String)> = sqlx::query_as(
        "SELECT u.id, u.handle, u.display_name, u.bio,
                (SELECT l.url FROM links l WHERE l.user_id = u.id AND l.hidden = false
                  ORDER BY l.position, l.created_at LIMIT 1) AS own_url,
                COALESCE((SELECT uv.vertical_slug FROM user_verticals uv
                  WHERE uv.user_id = u.id AND uv.role = 'primary' LIMIT 1), '') AS primary_vertical
         FROM users u
         WHERE u.hidden = false AND u.claimed_at IS NULL AND u.password_hash IS NULL
           AND NOT EXISTS (SELECT 1 FROM entity_people ep
                           WHERE ep.user_id = u.id AND ep.role = 'subject')
         ORDER BY u.created_at
         LIMIT $1",
    )
    .bind(limit.unwrap_or(i64::MAX))
    .fetch_all(pool)
    .await?;

    let mut counts = BackfillCounts {
        seen: rows.len() as i64,
        ..Default::default()
    };
    for (user_id, handle, title, bio, own_url, primary_vertical) in rows {
        let key = own_url.unwrap_or_else(|| format!("handle:{handle}"));
        let nsfw = primary_vertical == "nsfw";
        let id: Option<Uuid> = sqlx::query_scalar(
            "INSERT INTO entities
               (slug, kind, canonical_key, title, description, category_slug, nsfw,
                origin, source, source_id, imported_at)
             VALUES ($1, 'person', $2, $3, $4, NULLIF($5, ''), $6,
                     'import', 'registry', $7, now())
             ON CONFLICT (canonical_key) DO NOTHING RETURNING id",
        )
        .bind(entity_slug(&handle))
        .bind(key)
        .bind(title)
        .bind(bio)
        .bind(&primary_vertical)
        .bind(nsfw)
        .bind(&handle)
        .fetch_optional(pool)
        .await?;

        let Some(id) = id else {
            counts.conflicts += 1; // two shells sharing a first URL are one entity, not two
            continue;
        };
        counts.tags += sqlx::query(
            "INSERT INTO entity_tags (entity_id, tag_slug)
             SELECT $1, t.tag_slug FROM tags t
             WHERE t.user_id = $2
               AND t.tag_slug ~ '^[a-z0-9][a-z0-9 .-]{0,39}$'
             ON CONFLICT DO NOTHING",
        )
        .bind(id)
        .bind(user_id)
        .execute(pool)
        .await?
        .rows_affected() as i64;
        sqlx::query(
            "INSERT INTO entity_people (entity_id, user_id, role, claimed_at)
             VALUES ($1, $2, 'subject', NULL) ON CONFLICT DO NOTHING",
        )
        .bind(id)
        .bind(user_id)
        .execute(pool)
        .await?;
        counts.inserted += 1;
    }
    Ok(counts)
}
```

### 3. `src/ops.rs` — dry run, apply, receipt

Append:

```rust
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
```

Add a doctor line after the `fixtures` block (same `say` closure):

```rust
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
```

### 4. `src/main.rs` — the command

Add to `enum Cmd`:

```rust
    /// Fill the entity substrate from imported shells and write a receipt (idempotent).
    Catalogue {
        #[arg(long)]
        dry_run: bool,
        /// Stage a partial backfill; omit for all shells.
        #[arg(long)]
        limit: Option<i64>,
    },
```

Add to the dispatch match:

```rust
        Cmd::Catalogue { dry_run, limit } => {
            let pool = db::connect(&cfg).await?;
            orbital::ops::backfill_catalogue(&pool, limit, dry_run).await?;
        }
```

### 5. `tests/catalogue.rs` — the gate (new file)

```rust
//! M1's gate: the substrate fills once, a second run inserts nothing, accounts are never claimed
//! as somebody's subject, and a shared URL is one entity rather than two.

use sqlx::PgPool;
use uuid::Uuid;

async fn user(pool: &PgPool, handle: &str, with_password: bool) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO users (handle, display_name, password_hash) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(handle)
    .bind(handle)
    .bind(if with_password { Some("hash") } else { None })
    .fetch_one(pool)
    .await
    .expect("a user")
}

async fn seed(pool: &PgPool, id: Uuid, url: &str) {
    sqlx::query("INSERT INTO links (user_id, url, label) VALUES ($1, $2, 'site')")
        .bind(id)
        .bind(url)
        .execute(pool)
        .await
        .expect("a link");
    sqlx::query("INSERT INTO tags (user_id, tag_slug) VALUES ($1, 'rust'), ($1, 'bad+tag')")
        .bind(id)
        .execute(pool)
        .await
        .expect("two tags, one of which entity_tags refuses");
    sqlx::query("INSERT INTO user_verticals (user_id, vertical_slug, role) VALUES ($1, 'dev', 'primary')")
        .bind(id)
        .execute(pool)
        .await
        .expect("a primary vertical");
}

#[sqlx::test]
async fn backfill_fills_the_substrate_and_the_second_run_inserts_nothing(pool: PgPool) {
    sqlx::query("INSERT INTO verticals (slug, label) VALUES ('dev', 'Developers')")
        .execute(&pool).await.expect("a vertical");
    let shell = user(&pool, "sample-creator", false).await;
    let account = user(&pool, "real-person", true).await;
    seed(&pool, shell, "https://example.com/creator").await;
    seed(&pool, account, "https://example.com/real").await;

    let first = orbital::store::backfill_catalogue(&pool, None).await.expect("first run");
    assert_eq!(first.seen, 1, "accounts are not shells");
    assert_eq!(first.inserted, 1);
    assert_eq!(first.tags, 1, "bad+tag is refused by entity_tags' own CHECK");

    let second = orbital::store::backfill_catalogue(&pool, None).await.expect("second run");
    assert_eq!(second.seen, 0, "the second run's own INSERT 0 proves idempotency");
    assert_eq!(second.inserted, 0);

    let people: i64 = sqlx::query_scalar("SELECT count(*) FROM entities WHERE kind = 'person'")
        .fetch_one(&pool).await.expect("count");
    assert_eq!(people, 1);
    let categories: i64 = sqlx::query_scalar("SELECT count(*) FROM categories")
        .fetch_one(&pool).await.expect("count");
    assert_eq!(categories, 1, "categories were filled from verticals");
    let c = orbital::store::catalogue_composition(&pool).await.expect("composition");
    assert_eq!(c.shells, 1);
    assert_eq!(c.subjects, 1);
    assert_eq!(c.entities, 1);
}

#[sqlx::test]
async fn backfill_merges_two_shells_that_share_one_surface(pool: PgPool) {
    let a = user(&pool, "first-creator", false).await;
    let b = user(&pool, "second-creator", false).await;
    seed(&pool, a, "https://example.com/shared").await;
    seed(&pool, b, "https://example.com/shared").await;

    let c = orbital::store::backfill_catalogue(&pool, None).await.expect("run");
    assert_eq!(c.seen, 2);
    assert_eq!(c.inserted, 1);
    assert_eq!(c.conflicts, 1, "a shared own surface is one canonical entity");
}
```

**RED state before the code exists** (quoted from the missing symbols, not imagined output):
`error[E0425]: cannot find function 'backfill_catalogue' in module 'store'` and
`error[E0432]: unresolved import` for `tests/catalogue.rs`. After M1 the same command prints
`test result: ok. 2 passed; 0 failed`.

### Verify M1 (run on thinkcentre, never the workstation)

```bash
# Build/test host, test DB is docker lh-pg-test on 55433
ssh thinkcentre 'cd /personal/documents/code/rust/orbital \
  && export CARGO_TARGET_DIR=$HOME/.cargo-target/orbital \
  && export DATABASE_URL=postgres://darkhold:darkhold@127.0.0.1:55433/postgres \
  && cargo test --test catalogue'
# expects: running 2 tests ... test result: ok. 2 passed; 0 failed

ssh thinkcentre 'cd /personal/documents/code/rust/orbital \
  && export CARGO_TARGET_DIR=$HOME/.cargo-target/orbital \
  && cargo test --test catalogue -- --list | grep backfill'
# expects the two test names to be printed

cd ~/code-local/rust/orbital && ./scripts/deploy-thinkcentre.sh
# tracked background job; expects 7/7, doctor with a PASS catalogue line

ssh thinkcentre 'set -a; . ~/.config/systemd/user/orbital-db.env; set +a; \
  orbital catalogue --dry-run; orbital catalogue; orbital catalogue'
# expects: dry run N shell(s); then seen N inserted N; then seen 0 inserted 0

ssh thinkcentre 'set -a; . ~/.config/systemd/user/orbital-db.env; set +a; \
  psql "$DATABASE_URL" -tAc "select source,rows_seen,rows_inserted from import_runs order by id desc limit 2"'
# expects two catalogue-backfill rows, the newest 0|0

for u in https://orbital.polarisocial.xyz/ https://orbital.polarisocial.xyz/v/nsfw https://orbital.polarisocial.xyz/@thekayluxx; do
  curl -s -o /dev/null -w "%{http_code} $u\n" "$u"
done
# expects 200 three times
```

Host-tree precondition: `Cargo.lock` is modified and `scripts/creator-sources/enrich-new.json` is
untracked on thinkcentre; both abort the deploy's `git checkout -B main origin/main`. Commit the
lockfile locally, push to both remotes, then restore/remove those host files before deploying.

### M1 gate result so far (2026-11-16, real output)

- `cargo test --test catalogue` → `test result: ok. 2 passed; 0 failed; 0 ignored; ...`. Run on the
  workstation against the host's test DB (port 55433 via `ssh -L`) because the host's debug build sat
  in D state under I/O wait for 12 minutes with load >30. The verification checkout used
  `~/code-local/rust/commons` and `~/code/polaris-*` for the path deps: `~/code-local/polaris-eval` is
  stale and has no `paired` module, which is an environment mismatch, not a repo defect.
- The first real run exposed four defects that a passing test written against unfixed code would have
  hidden: `store.rs` has no `Result` alias (uses `anyhow::Result`), `own_url` decodes as
  `Option<String>` not `bool`, the `dev` vertical is seeded by migration 0001, and the seeded
  verticals produce 10 categories. All four were fixed and the rerun was green.
- SQL contract on a scratch DB with migrations 0001–0009 applied: first backfill inserted 1 entity
  (the shell, not the password-holding account), second inserted 0, composition read
  `shells=1, claimed=1, entities=1, subjects=1, receipts=0`, 1 tag survived the `entity_tags` CHECK,
  and categories were filled from verticals.
- Live deploy and the two `orbital catalogue` applies are **still running** when this is written.
  Do not mark M1 Done in `docs/ROADMAP.md` until their output is in the handoff.

---

## M2 — the entity read path (next; details written when M1 closes)

Home shelves first, then `/v/{slug}`, `/search`, the two feeds and `sitemap.xml`. One surface per
commit, each with a live 200/body probe and a roster-size assertion. The shelves keep their three
names and their per-source budgets; the switch is in `src/shelf.rs`, not scattered `.take(n)` edits.
A shell entity is visible as a catalogue entry with a claim CTA but is `noindex` and never ranked
until claimed. Gate: same-session before/after latency at the same load, plus every public surface
answering 200 with a non-empty list where a fixture provides one.

## M3 — claim proof and erasure

One claim path (page proof or Bluesky token), one `orbital erase <handle>` command with the same
reach as the importer, a 30-day conversion count, and a rollback rule fixed before more imports.
Gate: an unproved claim is refused; a proved one sets `users.claimed_at` and makes the entity
rankable; erasure removes the shell, its entity and its links in one transaction and a second run
reports 0.

## M4 — receipted ingestion

Fold `sweep.py`/`build.py`/`enrich.py`/`merge.py`/`crawl.py` into declared source rows and one runner
with per-source cursors, retry/backoff and an `import_runs` row per stage. Gate: two consecutive
runs, the second `INSERT 0 0` on every statement.

## M5 — scale contract and link health

Short-TTL roster cache until M2 removes the roster decode; link check budget ordered failing → new →
clicked, with coverage printed next to the failure count. Gate: same-session latency comparison and
a coverage number in `doctor`.

## M6 — evaluation before any new ranking rule

Commit a golden set over entities, run `orbital eval`, and refuse a ranking change below the sample
floor. Gate: `NO VERDICT` where the floor is not met; only `VERDICT: better` unlocks a change.

## Definition of done for this refactor

- `catalogue_composition` reports `shells <= subjects` on live.
- Every public discovery surface reads entities; no public page decodes the full member roster.
- No unclaimed shell ranks, is indexed, or appears in search or feeds.
- A claim with proof and a one-command erasure both have live gates.
- Every import run has a receipt and a second-run idempotency proof.
- `docs/ROADMAP.md` §4 has no open row without a gate that ran against the live host.
