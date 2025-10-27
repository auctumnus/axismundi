use axum::{
    extract::FromRequestParts,
    http::{StatusCode, request::Parts},
    response::{IntoResponse, Json},
};
use serde::{Deserialize, Serialize};

use crate::util::AppState;

type PaginationSize = i32;

const MAX_PAGE_SIZE: PaginationSize = 100;

#[derive(Serialize, Debug, Clone)]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub offset: PaginationSize,
    pub limit: PaginationSize,
    pub has_more: bool,
}

#[derive(Default, Deserialize, Debug, Clone)]
pub struct PaginatedRequest {
    #[serde(default = "default_limit")]
    pub limit: PaginationSize,
    #[serde(default)]
    pub offset: PaginationSize,
}

fn default_limit() -> PaginationSize {
    10
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

        if paginated_request.limit <= 0 || paginated_request.limit > MAX_PAGE_SIZE {
            return Err((StatusCode::BAD_REQUEST, "invalid limit parameter"));
        }

        if paginated_request.offset < 0 {
            return Err((StatusCode::BAD_REQUEST, "invalid offset parameter"));
        }

        Ok(paginated_request)
    }
}

impl<T: Serialize> IntoResponse for PaginatedResponse<T> {
    fn into_response(self) -> axum::response::Response {
        Json(self).into_response()
    }
}
