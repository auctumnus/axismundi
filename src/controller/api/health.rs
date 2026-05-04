use crate::util::{AppState, s3::S3};
use axum::{Json, extract::State, http::StatusCode};
use serde::Serialize;

pub fn create_router() -> axum::Router<AppState> {
    axum::Router::new().route("/health", axum::routing::get(health))
}

#[derive(Serialize)]
pub struct HealthResponse {
    status: &'static str,
    checks: Checks,
}

#[derive(Serialize)]
pub struct Checks {
    db: CheckStatus,
    s3: CheckStatus,
}

#[derive(Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Ok,
    Down,
}

async fn health(State(state): State<AppState>) -> (StatusCode, Json<HealthResponse>) {
    let db = match sqlx::query("SELECT 1").fetch_one(&state.pool).await {
        Ok(_) => CheckStatus::Ok,
        Err(e) => {
            tracing::warn!("health: db ping failed: {e}");
            CheckStatus::Down
        }
    };

    // any HTTP response (incl. 404) means the s3 endpoint is reachable.
    // only network errors and 5xx mean the dependency is actually down.
    // rust-s3 turns non-2xx responses into Err(HttpFailWithBody(status, _)),
    // so check that variant for 4xx (Ok) vs 5xx (Down).
    let s3 = match S3.bucket.head_object("_healthcheck/probe").await {
        Ok((_, status)) if status >= 500 => {
            tracing::warn!("health: s3 head_object returned {status}");
            CheckStatus::Down
        }
        Ok(_) => CheckStatus::Ok,
        Err(s3::error::S3Error::HttpFailWithBody(status, _)) if status < 500 => CheckStatus::Ok,
        Err(e) => {
            tracing::warn!("health: s3 ping failed: {e}");
            CheckStatus::Down
        }
    };

    // db is critical — degraded s3 still serves text content. caddy's
    // fallback page is for when the app process itself is gone.
    let http_status = if db == CheckStatus::Ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    let status = if db == CheckStatus::Ok && s3 == CheckStatus::Ok {
        "ok"
    } else {
        "degraded"
    };

    (
        http_status,
        Json(HealthResponse {
            status,
            checks: Checks { db, s3 },
        }),
    )
}
