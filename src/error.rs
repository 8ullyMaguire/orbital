use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use serde_json::json;

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
    pub fn status(&self) -> StatusCode {
        match self {
            AppError::Validation(_) => StatusCode::BAD_REQUEST,
            AppError::NotFound => StatusCode::NOT_FOUND,
            AppError::Unauthorized => StatusCode::UNAUTHORIZED,
            AppError::Forbidden => StatusCode::FORBIDDEN,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::Db(sqlx::Error::RowNotFound) => StatusCode::NOT_FOUND,
            AppError::Db(_) | AppError::Other(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            AppError::Validation(_) => "invalid",
            AppError::NotFound | AppError::Db(sqlx::Error::RowNotFound) => "not_found",
            AppError::Unauthorized => "unauthorized",
            AppError::Forbidden => "forbidden",
            AppError::Conflict(_) => "conflict",
            AppError::Db(_) | AppError::Other(_) => "internal",
        }
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
        let status = self.0.status();
        if status == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!(error = %self.0, "api error");
        }
        let body = json!({
            "error": {
                "code": self.0.code(),
                "message": self.0.to_string(),
            }
        });
        (status, axum::Json(body)).into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

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
