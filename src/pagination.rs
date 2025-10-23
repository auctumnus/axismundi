use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Json},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::util::AppState;

type PaginationSize = i32;

const MAX_PAGE_SIZE: PaginationSize = 100;

#[derive(Serialize, Debug, Clone)]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub pages_left: PaginationSize,
    // null if no next page
    pub next_cursor: Option<Uuid>,
    // null if no previous page
    pub previous_cursor: Option<Uuid>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct PaginatedRequest {
    #[serde(default = "default_limit")]
    pub limit: PaginationSize,
    pub cursor: Option<Uuid>,
    #[serde(default)]
    pub direction: PaginationDirection,
}

fn default_limit() -> PaginationSize {
    10
}

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum PaginationDirection {
    Forward,
    Backward,
}

impl Default for PaginationDirection {
    fn default() -> Self {
        PaginationDirection::Forward
    }
}

impl FromRequestParts<AppState> for PaginatedRequest {
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let query = parts.uri.query().unwrap_or("");
        let paginated_request: PaginatedRequest = serde_urlencoded::from_str(query)
            .map_err(|_| (StatusCode::BAD_REQUEST, "invalid pagination parameters"))?;

        if paginated_request.limit == 0 || paginated_request.limit > MAX_PAGE_SIZE {
            return Err((StatusCode::BAD_REQUEST, "invalid limit parameter"));
        }

        Ok(paginated_request)
    }
}

impl<T: Serialize> IntoResponse for PaginatedResponse<T> {
    fn into_response(self) -> axum::response::Response {
        Json(self).into_response()
    }
}
