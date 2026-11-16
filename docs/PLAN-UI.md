# PLAN-UI — public surface quality and the estate error contract (2026-11-16)

Source of truth for the problem and the acceptance criteria: `docs/SPEC.md` §8.
This plan changes presentation plus the `/api/*` error serializer. It does not touch ranking,
catalogue data, claims, erasure or imports.

## Task 1 — adopt `polaris-core` for API errors

Files: `Cargo.toml`, `src/error.rs`, `tests/api_error_contract.rs`.

1. Add the dependency beside the other estate crates:

```toml
# The estate's error envelope: closed code set, 300-char message cap, status/code agreement.
polaris-core = { path = "../../polaris-core-rust" }
```

2. In `src/error.rs`, keep `AppError` (it carries this product's meaning) and delegate the wire
   shape. Replace `status()`/`code()` with one mapping:

```rust
use polaris_core::{ApiError as EstateApiError, ErrorCode};

impl AppError {
    pub fn code(&self) -> ErrorCode {
        match self {
            AppError::Validation(_) => ErrorCode::InvalidRequest,
            AppError::NotFound | AppError::Db(sqlx::Error::RowNotFound) => ErrorCode::NotFound,
            AppError::Unauthorized => ErrorCode::Unauthenticated,
            AppError::Forbidden => ErrorCode::Forbidden,
            AppError::Conflict(_) => ErrorCode::Conflict,
            AppError::Db(_) | AppError::Other(_) => ErrorCode::Internal,
        }
    }

    pub fn status(&self) -> StatusCode {
        self.code().default_status()
    }
}
```

3. `ApiError::into_response` becomes:

```rust
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let code = self.0.code();
        let status = code.default_status();
        if status == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!(error = %self.0, "api error");
        }
        let message = self.0.to_string();
        let body = match code {
            ErrorCode::InvalidRequest => EstateApiError::invalid_request(message),
            ErrorCode::NotFound => EstateApiError::not_found(message),
            ErrorCode::Unauthenticated => EstateApiError::unauthenticated(message),
            ErrorCode::Forbidden => EstateApiError::forbidden(message),
            ErrorCode::Conflict => EstateApiError::conflict(message),
            _ => EstateApiError::internal(message),
        };
        body.into_response()
    }
}
```

Remove the hand-built `serde_json::json!` envelope. `PageError` keeps its HTML rendering and can
use `self.0.status()` as before.

4. `tests/api_error_contract.rs`:

```rust
use axum::body::to_bytes;
use axum::response::IntoResponse;
use orbital::error::{ApiError, AppError};
use serde_json::{json, Value};

async fn body(err: AppError) -> (u16, Value) {
    let res = ApiError(err).into_response();
    let status = res.status().as_u16();
    let bytes = to_bytes(res.into_body(), 4096).await.expect("body");
    (status, serde_json::from_slice(&bytes).expect("json"))
}

#[tokio::test]
async fn validation_is_invalid_request() {
    let (status, body) = body(AppError::Validation("no such vertical".into())).await;
    assert_eq!(status, 400);
    assert_eq!(body, json!({"error":{"code":"invalid_request","message":"no such vertical"}}));
}

#[tokio::test]
async fn signed_out_is_unauthenticated() {
    let (status, body) = body(AppError::Unauthorized).await;
    assert_eq!(status, 401);
    assert_eq!(body["error"]["code"], "unauthenticated");
}

#[tokio::test]
async fn missing_is_not_found() {
    let (status, body) = body(AppError::NotFound).await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "not_found");
}

#[tokio::test]
async fn internal_never_leaks_a_stack() {
    let (status, body) = body(AppError::Other(anyhow::anyhow!("database password"))).await;
    assert_eq!(status, 500);
    assert_eq!(body["error"]["code"], "internal");
    assert!(!body.to_string().contains("password"));
}
```

Verify: `cargo test --test api_error_contract` → `4 passed`.

## Task 2 — one page frame and a phone menu

Files: `templates/base.html`, `static/site.css`.

1. Add the mobile disclosure in `templates/base.html`, inside `.top-r` before the language link:

```html
<details class="mnav">
  <summary class="nav-a mnav-summary">Menu</summary>
  <div class="drop-panel mnav-panel">
    <a class="drop-i" href="/">{{ layout.t("nav.directory") }}</a>
    {% for n in layout.nav %}<a class="drop-i" href="/v/{{ n.slug }}">{{ n.icon|safe }}<span>{{ n.label }}</span></a>{% endfor %}
    <a class="drop-i" href="/search">{{ layout.t("nav.search") }}</a>
  </div>
</details>
```

2. Change the footer column headings from `<h4>` to `<p class="foot-h">` in all three columns.
   This removes the `h1 → h4` skip on every page without inventing a second page heading.

3. Add to `<head>`:

```html
<meta name="theme-color" content="#06070d">
<link rel="manifest" href="/assets/manifest.webmanifest">
```

4. CSS:

```css
:root { color-scheme: dark; }
.brand { min-height: 44px; }
.nav-a { min-height: 44px; display: inline-flex; align-items: center; padding: 10px 12px; }
.btn-sm { min-height: 44px; }
.drop-i { min-height: 44px; }
.mnav { display: none; position: relative; }
.mnav > summary { list-style: none; cursor: pointer; }
.mnav > summary::-webkit-details-marker { display: none; }
.mnav-panel { left: auto; right: 0; }
.foot-h { font: 600 .74rem/1 var(--mono); letter-spacing: .1em; text-transform: uppercase; color: var(--dim); margin: 0 0 10px; }
@media (max-width: 680px) { .nav { display: none; } .mnav { display: block; } }
```

5. `static/manifest.webmanifest`:

```json
{
  "name": "Orbital — A directory of people",
  "short_name": "Orbital",
  "description": "A directory of humans and their links, sorted into verticals.",
  "start_url": "/",
  "display": "standalone",
  "background_color": "#06070d",
  "theme_color": "#06070d",
  "icons": [
    { "src": "/assets/favicon.svg", "sizes": "any", "type": "image/svg+xml", "purpose": "any" }
  ]
}
```

Verify: `cargo test --test ui_contract` and the live probe in Task 5.

## Task 3 — hero grid, avatar ink, catalogue cards

Files: `static/site.css`, `src/html.rs`, `templates/home.html`, `templates/search.html`.

1. `.hero` uses the page width, not a second grid:

```css
.hero { padding: 52px 24px 30px; max-width: var(--max); }
```

The existing `.lede { max-width: 620px }` and `.searchrow { max-width: 560px }` keep the readable
measure. Mobile keeps its `padding-top: 54px` override.

2. Generated avatar: white ink over a dark scrim so every accent clears normal-text contrast:

```css
.av-gen {
  display: inline-grid; place-items: center; font-weight: 700; color: #fff;
  background:
    linear-gradient(140deg, #06070d99, #06070d99),
    linear-gradient(140deg, var(--accent) 0%, var(--accent2) 100%);
  letter-spacing: -0.02em;
}
```

Add `aria-hidden="true"` to the generated `<span>` in `src/html.rs::avatar`; the name is always
adjacent text, so the initials are decorative and must not be announced twice.

3. Catalogue cards become `<article class="vcard">` with a title link, not a link wrapping the card:

```html
<article class="vcard">
  <div class="vcard-h">
    <a class="vname" href="{{ e.url }}" {% if !e.claimed %}rel="nofollow"{% endif %}>{{ e.title }}</a>
    <span class="pill">{{ e.category_label }}</span>
  </div>
  <p class="muted sm vdesc">{{ e.description }}</p>
  {% if !e.claimed %}<p class="muted sm">Unclaimed — <a class="btn btn-ghost btn-sm" href="/app/#/join">Claim this</a></p>{% endif %}
</article>
```

Apply the same change in `templates/search.html`.

4. Shorten the placeholder in `home.html` and `search.html` to
   `Name, handle, tag, or site` so it fits the 390px search row.

5. Heading order: change `.empty h3` to `<h2>` in `vertical.html` and `search.html`; keep
   `.section-h h2` as the page section heading. On `/v/nsfw` the order becomes `[1,2]` plus footer
   text (no headings); on `/search` it becomes `[1,2,2,...]`.

Verify: `cargo test --test ui_contract` → `4 passed`; `cargo test --test http` → `69+ passed`.

## Task 4 — keep the audit

Files: `scripts/site-audit/probe.js`, `scripts/site-audit/sweep.json`.

Copy the browser probe from the `whole-site-visual-audit` skill into the repo and commit a route
list: `/`, `/v/nsfw`, `/search?q=patreon`, `/about`, `/does-not-exist`. The next pass is then a
re-run, not a rewrite.

## Task 5 — live gate

1. `cargo test --test api_error_contract` → 4 passed.
2. `cargo test --test ui_contract` → 4 passed.
3. `cargo test --test http` → all passed.
4. Build and install the release binary, restart the user unit.
5. Browser probe at 1280 and 390 on the three routes: overflow 0, main non-empty, no header target
   under 44, `.mnav` visible and `.nav` hidden at 390, hero h1 left equals section left at 1280.
6. Inject axe-core from a pinned CDN in the real browser and run the WCAG 2.1 AA tag set on the
   three routes; record `violations`, `passes`, `incomplete`.
7. `curl -sI -m 20 https://orbital.polarisocial.xyz/assets/manifest.webmanifest` → 200
   `application/manifest+json`.
8. `curl` `/`, `/v/nsfw`, `/search?q=patreon`, `/feed.json` → 200; `orbital doctor | grep -E
   'catalogue|link coverage'` unchanged.

## Rollback

Each task is one commit. `git revert` the UI commit restores the old templates and CSS; the
`polaris-core` commit is additive except for `src/error.rs`, and `Cargo.lock` is in the same commit.
No migration, no data change, no deploy step that cannot be repeated.

## Outcome (2026-11-16)

Implemented with three additions the plan did not name: `api_envelope` normalises Axum's own
rejections (bad JSON, wrong method, unknown API path) rather than only handler errors; inline prose
links gained an underline after axe's `link-in-text-block` finding on `/search`; and the breadcrumb
became a 44 px target. `api_error_contract` finished at **7 passed**, not 4; `ui_contract` 4 and
`http` 69 unchanged. The version query is `?v=2026.11.16.4` on the stylesheet and manifest so a
deploy is not hidden behind the edge's five-minute cache. Full measurements: `docs/SPEC.md` §8
"Result".
