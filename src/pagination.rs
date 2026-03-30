use askama::Template;
use axum::{
    extract::FromRequestParts,
    http::{StatusCode, request::Parts},
    response::{IntoResponse, Json},
};
use serde::{Deserialize, Serialize};
use tokio::stream;

use crate::util::{AppState, serialize_search};

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

impl<T> PaginatedResponse<T> {
    pub fn request_last_page(&self) -> PaginatedRequest {
        let last_offset = if self.total % i64::from(self.limit) == 0 {
            self.total - i64::from(self.limit)
        } else {
            self.total - (self.total % i64::from(self.limit))
        };
        PaginatedRequest {
            limit: self.limit,
            offset: last_offset.try_into().unwrap_or(0),
        }
    }

    pub fn total_pages(&self) -> i32 {
        ((self.total + i64::from(self.limit) - 1) / i64::from(self.limit))
            .try_into()
            .unwrap_or(0)
    }

    pub fn current_page(&self) -> i32 {
        (self.offset / self.limit) + 1
    }

    pub fn results_text(&self) -> String {
        if self.total == 1 {
            format!("{} result found", self.total)
        } else {
            format!("{} results found", self.total)
        }
    }

    pub fn map<U, F: FnMut(T) -> U>(self, f: F) -> PaginatedResponse<U> {
        PaginatedResponse {
            items: self.items.into_iter().map(f).collect(),
            total: self.total,
            offset: self.offset,
            limit: self.limit,
            has_more: self.has_more,
        }
    }

    pub async fn map_async<U, F, Fut>(self, mut f: F) -> PaginatedResponse<U>
    where
        F: FnMut(T) -> Fut,
        Fut: std::future::Future<Output = U>,
    {
        let mut set = tokio::task::JoinSet::new();
        for (i, item) in self.items.into_iter().enumerate() {
            set.spawn(async move {
                let result = f(item).await;
                (i, result)
            });
        }
        let mut results = vec![None; set.len()];
        while let Some(res) = set.join_next().await {
            if let Ok((i, result)) = res {
                results[i] = Some(result);
            }
        }

        PaginatedResponse {
            items,
            total: self.total,
            offset: self.offset,
            limit: self.limit,
            has_more: self.has_more,
        }
    }
}

#[derive(Deserialize, Debug, Clone, Serialize)]
pub struct PaginatedRequest {
    #[serde(default = "default_limit")]
    pub limit: PaginationSize,
    #[serde(default)]
    pub offset: PaginationSize,
}

impl PaginatedRequest {
    pub fn with_previous_page(&self) -> Self {
        let new_offset = (self.offset - self.limit).max(0);
        Self {
            limit: self.limit,
            offset: new_offset,
        }
    }

    pub fn with_next_page(&self) -> Self {
        Self {
            limit: self.limit,
            offset: self.offset + self.limit,
        }
    }
}

fn default_limit() -> PaginationSize {
    10
}

impl Default for PaginatedRequest {
    fn default() -> Self {
        Self {
            limit: default_limit(),
            offset: 0,
        }
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

#[derive(Template)]
#[template(path = "pagination.html")]
pub struct PaginationTemplate {
    pub first_page: String,
    pub previous_page: String,
    pub current_page: i32,
    pub total_pages: i32,
    pub next_page: String,
    pub last_page: String,
    pub results_text: String,
    pub has_more: bool,
    pub has_prev: bool,
}

impl PaginationTemplate {
    pub fn from_paginated_response<T, Q: Serialize>(
        base_url: &str,
        response: &PaginatedResponse<T>,
        pagination: &PaginatedRequest,
        query: Q,
    ) -> Self {
        let first_search = serde_urlencoded::to_string(&query).unwrap_or_default();
        let first_page = format!("{base_url}?{first_search}");

        let previous_search = serialize_search(&pagination.with_previous_page(), &query);
        let previous_page = format!("{base_url}?{previous_search}");

        let next_search = serialize_search(&pagination.with_next_page(), &query);
        let next_page = format!("{base_url}?{next_search}");

        let last_search = serialize_search(&response.request_last_page(), &query);
        let last_page = format!("{base_url}?{last_search}");

        Self {
            first_page,
            previous_page,
            current_page: response.current_page(),
            total_pages: response.total_pages(),
            next_page,
            last_page,
            results_text: response.results_text(),
            has_more: response.has_more,
            has_prev: pagination.offset > 0,
        }
    }
}