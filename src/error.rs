use axum::body::to_bytes;
use axum::http::header::CONTENT_TYPE;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use polaris_core::{ApiError as EstateApiError, ErrorCode};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Validation(String),
    #[error("not found")]
    NotFound,
    #[error("sign in first")]
    Unauthorized,
    #[error("not allowed")]
    Forbidden,
    #[error("{0}")]
    Conflict(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl AppError {
    /// This product's meanings, mapped once onto the estate's closed code set.
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

/// JSON errors for `/api/*`.
pub struct ApiError(pub AppError);

impl From<AppError> for ApiError {
    fn from(e: AppError) -> Self {
        ApiError(e)
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        ApiError(AppError::Db(e))
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        ApiError(AppError::Other(e))
    }
}

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
            // A 500's cause belongs in the log, not in a response a stranger can read.
            _ => EstateApiError::internal("internal error"),
        };
        body.into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

/// Normalise the responses Axum itself rejects (bad JSON, no route, wrong method) into the same
/// envelope as the handlers. Extractor rejections are text/plain by default; an API that speaks the
/// contract only when its own code fails is not speaking it.
pub async fn api_envelope(res: Response) -> Response {
    let status = res.status();
    let already_json = res
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("application/json"));
    if status.is_success() || status.is_redirection() || already_json {
        return res;
    }

    let detail = to_bytes(res.into_body(), 8 * 1024)
        .await
        .ok()
        .and_then(|b| String::from_utf8(b.to_vec()).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let message = detail.unwrap_or_else(|| {
        status
            .canonical_reason()
            .unwrap_or("request failed")
            .to_lowercase()
    });

    let code = match status {
        StatusCode::UNAUTHORIZED => ErrorCode::Unauthenticated,
        StatusCode::FORBIDDEN => ErrorCode::Forbidden,
        StatusCode::NOT_FOUND => ErrorCode::NotFound,
        StatusCode::METHOD_NOT_ALLOWED => ErrorCode::MethodNotAllowed,
        StatusCode::CONFLICT => ErrorCode::Conflict,
        StatusCode::GONE => ErrorCode::Gone,
        StatusCode::UNPROCESSABLE_ENTITY => ErrorCode::Unprocessable,
        StatusCode::TOO_MANY_REQUESTS => ErrorCode::RateLimited,
        StatusCode::SERVICE_UNAVAILABLE => ErrorCode::Unavailable,
        s if s.is_client_error() => ErrorCode::InvalidRequest,
        _ => ErrorCode::Internal,
    };
    let body = match code {
        ErrorCode::InvalidRequest => EstateApiError::invalid_request(message),
        ErrorCode::Unauthenticated => EstateApiError::unauthenticated(message),
        ErrorCode::Forbidden => EstateApiError::forbidden(message),
        ErrorCode::NotFound => EstateApiError::not_found(message),
        ErrorCode::MethodNotAllowed => EstateApiError::method_not_allowed(message),
        ErrorCode::Conflict => EstateApiError::conflict(message),
        ErrorCode::Gone => EstateApiError::gone(message),
        ErrorCode::Unprocessable => EstateApiError::unprocessable(message),
        ErrorCode::RateLimited => EstateApiError::rate_limited(message, 60),
        ErrorCode::Unavailable => EstateApiError::unavailable(message),
        // A 500's cause belongs in the log, not in a response a stranger can read.
        _ => EstateApiError::internal("internal error"),
    };
    body.into_response()
}

/// HTML errors for rendered pages.
pub struct PageError(pub AppError);

impl From<AppError> for PageError {
    fn from(e: AppError) -> Self {
        PageError(e)
    }
}

impl From<sqlx::Error> for PageError {
    fn from(e: sqlx::Error) -> Self {
        PageError(AppError::Db(e))
    }
}

impl From<anyhow::Error> for PageError {
    fn from(e: anyhow::Error) -> Self {
        PageError(AppError::Other(e))
    }
}

impl IntoResponse for PageError {
    fn into_response(self) -> Response {
        let status = self.0.status();
        if status == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!(error = %self.0, "page error");
        }
        let (title, body) = match &self.0 {
            AppError::NotFound => (
                "Not found",
                "That page does not exist. The directory is at the front door.".to_string(),
            ),
            AppError::Unauthorized => (
                "Sign in first",
                "This page needs an account. Signing up takes one handle and one password."
                    .to_string(),
            ),
            AppError::Forbidden => ("Not allowed", "This account cannot do that.".to_string()),
            AppError::Validation(m) => ("That is not valid", m.clone()),
            AppError::Conflict(m) => ("Already taken", m.clone()),
            _ => (
                "Something broke",
                "The server could not finish that. It has been logged.".to_string(),
            ),
        };
        let html = crate::html::error_page(status.as_u16(), title, &body);
        (status, Html(html)).into_response()
    }
}

pub type PageResult<T> = Result<T, PageError>;
