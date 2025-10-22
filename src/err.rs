use std::fmt::Display;

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

use crate::config;

#[derive(Debug, Clone)]
pub struct AppError {
    pub message: String,
    pub status_code: StatusCode,
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    pub const fn new(message: String, status_code: StatusCode) -> Self {
        Self {
            message,
            status_code,
        }
    }
}

pub fn not_found(item: impl Display) -> AppError {
    AppError::new(format!("{item} not found"), StatusCode::NOT_FOUND)
}

pub fn bad_request(message: impl Display) -> AppError {
    AppError::new(message.to_string(), StatusCode::BAD_REQUEST)
}

pub fn internal_error(message: impl Display) -> AppError {
    if config::CONFIG.environment == config::Environment::Dev {
        AppError::new(message.to_string(), StatusCode::INTERNAL_SERVER_ERROR)
    } else {
        generic_error()
    }
}

pub fn generic_error() -> AppError {
    AppError::new(
        "internal server error".to_string(),
        StatusCode::INTERNAL_SERVER_ERROR,
    )
}

pub fn unauthorized_no_session() -> AppError {
    AppError::new("no session found".to_string(), StatusCode::UNAUTHORIZED)
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.status_code, self.message)
    }
}

unsafe impl Send for AppError {}
unsafe impl Sync for AppError {}

impl core::error::Error for AppError {}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        Self {
            message: err.to_string(),
            status_code: StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl From<validator::ValidationErrors> for AppError {
    fn from(errors: validator::ValidationErrors) -> Self {
        Self {
            message: errors.to_string(),
            status_code: StatusCode::BAD_REQUEST,
        }
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => Self {
                message: "Resource not found".to_string(),
                status_code: StatusCode::NOT_FOUND,
            },
            _ => internal_error(err),
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        internal_error(err)
    }
}

impl From<image::ImageError> for AppError {
    fn from(err: image::ImageError) -> Self {
        internal_error(err)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (self.status_code, self.message).into_response()
    }
}
