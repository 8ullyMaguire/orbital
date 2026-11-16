//! The public shell is a contract too: one page frame, a phone menu, valid card markup and a
//! manifest. These are template and rendered-page invariants, not a screenshot review.

use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt;
use orbital::config::Config;
use orbital::AppState;
use sqlx::PgPool;
use tower::ServiceExt;

async fn home(pool: PgPool) -> String {
    let state = AppState {
        pool,
        cfg: std::sync::Arc::new(Config::for_test("http://orbital.test")),
        http: reqwest::Client::new(),
    };
    let app = orbital::build_router(state).await;
    let res = app
        .oneshot(Request::get("/").body(Body::empty()).expect("request"))
        .await
        .expect("router answered");
    assert_eq!(res.status(), 200);
    let bytes = res.into_body().collect().await.expect("body").to_bytes();
    String::from_utf8_lossy(&bytes).to_string()
}

#[sqlx::test]
async fn home_renders_one_h1_and_the_mobile_shell(pool: PgPool) {
    let body = home(pool).await;
    assert_eq!(body.matches("<h1").count(), 1, "one h1 per page");
    assert!(body.contains(r#"<meta name="theme-color""#));
    assert!(body.contains(r#"<link rel="manifest" href="/assets/manifest.webmanifest?v=2026.11.16.2">"#));
    assert!(body.contains(r#"<details class="mnav">"#));
    assert!(!body.contains("<h4"), "footer labels are not a fourth heading level");
}

#[test]
fn catalogue_cards_do_not_nest_anchors() {
    for tpl in [
        include_str!("../templates/home.html"),
        include_str!("../templates/search.html"),
    ] {
        assert!(
            !tpl.contains(r#"<a class="vcard" href="{{ e.url }}""#),
            "an anchor wrapping a catalogue card cannot contain the claim anchor"
        );
        assert!(tpl.contains(r#"<article class="vcard">"#));
        assert!(tpl.contains(r#"class="btn btn-ghost btn-sm" href="/app/#/join""#));
    }
}

#[test]
fn base_has_a_manifest_and_a_phone_menu() {
    let base = include_str!("../templates/base.html");
    assert!(base.contains(r#"<link rel="manifest" href="/assets/manifest.webmanifest?v=2026.11.16.2">"#));
    assert!(base.contains(r#"<details class="mnav">"#));
    assert!(base.contains(r#"<summary class="nav-a mnav-summary" aria-label="Menu""#));
}

#[test]
fn footer_headings_are_labels_not_headings() {
    let base = include_str!("../templates/base.html");
    assert!(!base.contains("<h4"));
    assert_eq!(base.matches(r#"<p class="foot-h">"#).count(), 3);
}
