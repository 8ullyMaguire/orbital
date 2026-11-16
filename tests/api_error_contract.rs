//! The /api/* refusals are the estate contract, not an orbital-shaped near miss.
//!
//! The shared crate owns the closed code set and the 300-character cap; these tests assert the
//! product's mapping onto it, not the crate's own arithmetic.

use axum::body::to_bytes;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use orbital::config::Config;
use orbital::error::{api_envelope, ApiError, AppError};
use orbital::AppState;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;

async fn envelope(res: Response) -> (u16, Value) {
    let res = api_envelope(res).await;
    let status = res.status().as_u16();
    let bytes = to_bytes(res.into_body(), 4096).await.expect("body");
    (status, serde_json::from_slice(&bytes).expect("json envelope"))
}

async fn body(err: AppError) -> (u16, Value) {
    let res = ApiError(err).into_response();
    let status = res.status().as_u16();
    let bytes = to_bytes(res.into_body(), 4096).await.expect("body");
    (status, serde_json::from_slice(&bytes).expect("json envelope"))
}

#[tokio::test]
async fn validation_is_invalid_request() {
    let (status, body) = body(AppError::Validation("no such vertical".into())).await;
    assert_eq!(status, 400);
    assert_eq!(
        body,
        json!({"error": {"code": "invalid_request", "message": "no such vertical"}})
    );
}

#[tokio::test]
async fn signed_out_is_unauthenticated() {
    let (status, body) = body(AppError::Unauthorized).await;
    assert_eq!(status, 401);
    assert_eq!(body["error"]["code"], "unauthenticated");
    assert_eq!(body["error"]["message"], "sign in first");
}

#[tokio::test]
async fn missing_is_not_found() {
    let (status, body) = body(AppError::NotFound).await;
    assert_eq!(status, 404);
    assert_eq!(body["error"]["code"], "not_found");
}

#[tokio::test]
async fn internal_never_leaks_the_cause() {
    let (status, body) = body(AppError::Other(anyhow::anyhow!("database password"))).await;
    assert_eq!(status, 500);
    assert_eq!(body["error"]["code"], "internal");
    assert!(!body.to_string().contains("password"));
}

#[tokio::test]
async fn an_axum_body_rejection_speaks_the_envelope() {
    let raw = (
        StatusCode::UNPROCESSABLE_ENTITY,
        "Failed to deserialize the JSON body: missing field `handle`",
    )
        .into_response();
    let (status, body) = envelope(raw).await;
    assert_eq!(status, 400);
    assert_eq!(body["error"]["code"], "unprocessable");
    assert!(body["error"]["message"]
        .as_str()
        .expect("message")
        .contains("missing field"));
}

#[tokio::test]
async fn a_route_that_does_not_exist_is_not_an_empty_404() {
    let (status, body) = envelope(StatusCode::NOT_FOUND.into_response()).await;
    assert_eq!(status, 404);
    assert_eq!(body, json!({"error": {"code": "not_found", "message": "not found"}}));
}

#[sqlx::test]
async fn an_unmatched_api_path_answers_with_the_envelope(pool: PgPool) {
    let state = AppState {
        pool,
        cfg: std::sync::Arc::new(Config::for_test("http://orbital.test")),
        http: reqwest::Client::new(),
    };
    let app = orbital::build_router(state).await;
    let res = app
        .oneshot(
            axum::http::Request::get("/api/v1/nope")
                .body(axum::body::Body::empty())
                .expect("request"),
        )
        .await
        .expect("router answered");
    assert_eq!(res.status(), 404);
    let bytes = to_bytes(res.into_body(), 4096).await.expect("body");
    let body: Value = serde_json::from_slice(&bytes).expect("json envelope");
    assert_eq!(body["error"]["code"], "not_found");
}
