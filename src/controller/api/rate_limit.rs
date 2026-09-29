use std::time::Duration;

use axum::{
    body::Body,
    http::{HeaderValue, Response, StatusCode, header},
};
use governor::middleware::NoOpMiddleware;
use tower_governor::{
    GovernorError,
    governor::{GovernorConfig, GovernorConfigBuilder},
    key_extractor::PeerIpKeyExtractor,
};

pub(super) type ApiGovernorConfig = GovernorConfig<PeerIpKeyExtractor, NoOpMiddleware>;

/// Bursts of eight requests, one more every 500ms (tower_governor's default).
#[cfg_attr(test, allow(dead_code))]
pub(super) fn normal_config() -> ApiGovernorConfig {
    config(Duration::from_millis(500), 8)
}

/// Bursts of two requests, one more every four seconds (tower_governor's `secure()`).
#[cfg_attr(test, allow(dead_code))]
pub(super) fn secure_config() -> ApiGovernorConfig {
    config(Duration::from_secs(4), 2)
}

fn config(period: Duration, burst_size: u32) -> ApiGovernorConfig {
    GovernorConfigBuilder::default()
        .period(period)
        .burst_size(burst_size)
        .error_handler(error_response)
        .finish()
        .expect("rate limit period and burst size are non-zero")
}

fn error_response(error: GovernorError) -> Response<Body> {
    let wait_time = match error {
        GovernorError::TooManyRequests { wait_time, .. } => wait_time,
        mut other => return other.as_response(),
    };

    // tower_governor truncates the remaining wait to whole seconds, so a
    // 400ms wait arrives here as 0. Round up so clients never see
    // `Retry-After: 0` and retry straight into another 429.
    let retry_after = wait_time + 1;
    let unit = if retry_after == 1 { "second" } else { "seconds" };

    let mut response = Response::new(Body::from(format!(
        "Too many requests. Retry after {retry_after} {unit}."
    )));
    *response.status_mut() = StatusCode::TOO_MANY_REQUESTS;
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    headers.insert(header::RETRY_AFTER, retry_after.into());
    headers.insert("x-ratelimit-after", retry_after.into());
    response
}

#[cfg(test)]
mod tests {
    use std::{net::SocketAddr, sync::Arc};

    use axum::{Router, extract::ConnectInfo, http::Request, routing::get};
    use tower::ServiceExt as _;

    use super::*;

    async fn hit(app: &Router) -> Response<Body> {
        let mut req = Request::builder().uri("/").body(Body::empty()).unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 1234))));
        app.clone().oneshot(req).await.unwrap()
    }

    #[tokio::test]
    async fn sub_second_wait_reports_one_second() {
        let app = Router::new().route("/", get(|| async { "ok" })).layer(
            tower_governor::GovernorLayer {
                config: Arc::new(config(Duration::from_millis(500), 1)),
            },
        );

        assert_eq!(hit(&app).await.status(), StatusCode::OK);
        let limited = hit(&app).await;

        assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(limited.headers()[header::RETRY_AFTER], "1");
        assert_eq!(limited.headers()["x-ratelimit-after"], "1");
        let body = axum::body::to_bytes(limited.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body, "Too many requests. Retry after 1 second.");
    }

    #[test]
    fn multi_second_wait_rounds_up() {
        let response = error_response(GovernorError::TooManyRequests {
            wait_time: 3,
            headers: None,
        });

        assert_eq!(response.headers()[header::RETRY_AFTER], "4");
    }
}
