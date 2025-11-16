use std::fmt::Display;

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use validator::ValidationError;

use crate::config;

#[derive(Debug, Clone)]
pub struct AppError {
    pub message: String,
    pub status_code: StatusCode,
    pub validation_errors: Option<validator::ValidationErrors>,
}

pub type AppResult<T> = Result<T, AppError>;

fn prettify_validation_error(error: &ValidationError) -> String {
    if let Some(message) = &error.message {
        message.to_string()
    } else {
        match error.code.as_ref() {
            "length" => {
                let min = error.params.get("min").and_then(serde_json::Value::as_u64);
                let max = error.params.get("max").and_then(serde_json::Value::as_u64);
                if let (Some(min), Some(max)) = (min, max) {
                    format!("length must be between {} and {}", min, max)
                } else if let Some(min) = min {
                    format!("length must be at least {}", min)
                } else if let Some(max) = max {
                    format!("length must be at most {}", max)
                } else {
                    "invalid length".to_string()
                }
            },
            "regex" => "value is incorrectly formatted".to_string(),
            "email" => "invalid email format".to_string(),
            "password_strength" => "password is too weak".to_string(),
            _ => "invalid value".to_string(),
        }
    }
}

impl AppError {
    pub const fn new(message: String, status_code: StatusCode) -> Self {
        Self {
            message,
            status_code,
            validation_errors: None,
        }
    }

    pub fn error_for_field(&self, field: &str) -> Option<Vec<String>> {
        if let Some(errors) = &self.validation_errors {
            if let Some(error) = errors.field_errors().get(field) {
                if !error.is_empty() {
                    let error = error.iter()
                        .map(prettify_validation_error)
                        .collect();

                    return Some(error)
                }
            }
        }
        None
    }

    pub fn top_level_error(&self) -> Option<&str> {
        match self.validation_errors {
            None => Some(&self.message),
            Some(_) => None,
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
        eprintln!("Internal error: {message}");
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

pub fn forbidden(message: impl Display) -> AppError {
    AppError::new(message.to_string(), StatusCode::FORBIDDEN)
}

pub fn needs_verification() -> AppError {
    AppError::new(
        "email verification required".to_string(),
        StatusCode::FORBIDDEN,
    )
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
            validation_errors: None,
        }
    }
}

impl From<validator::ValidationErrors> for AppError {
    fn from(errors: validator::ValidationErrors) -> Self {
        Self {
            message: errors.to_string(),
            status_code: StatusCode::BAD_REQUEST,
            validation_errors: Some(errors),
        }
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => Self {
                message: "Resource not found".to_string(),
                status_code: StatusCode::NOT_FOUND,
                validation_errors: None,
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
