use axum::{Router, routing::get};
use axum_prometheus::metrics_exporter_prometheus::PrometheusHandle;
use std::net::SocketAddr;

fn page_size() -> u64 {
    // SAFETY: sysconf with _SC_PAGESIZE is always safe.
    let v = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if v <= 0 { 4096 } else { v as u64 }
}

fn read_rss_bytes() -> u64 {
    // /proc/self/statm has 7 space-separated fields. field 2 is "resident"
    // (number of pages currently in RAM). multiply by page size for bytes.
    let s = std::fs::read_to_string("/proc/self/statm").unwrap_or_default();
    let pages: u64 = s
        .split_whitespace()
        .nth(1)
        .and_then(|t| t.parse().ok())
        .unwrap_or(0);
    pages * page_size()
}

fn render_metrics(handle: &PrometheusHandle) -> String {
    let mut out = handle.render();
    out.push_str("# HELP axismundi_process_resident_memory_bytes Resident set size of the app process in bytes.\n");
    out.push_str("# TYPE axismundi_process_resident_memory_bytes gauge\n");
    out.push_str(&format!(
        "axismundi_process_resident_memory_bytes {}\n",
        read_rss_bytes()
    ));
    out
}

pub fn serve_metrics(handle: PrometheusHandle, port: u16) {
    if port == 0 {
        tracing::debug!("metrics_port = 0, /metrics endpoint disabled");
        return;
    }
    tokio::spawn(async move {
        let router = Router::new().route(
            "/metrics",
            get(move || {
                let h = handle.clone();
                async move { render_metrics(&h) }
            }),
        );
        let addr = SocketAddr::from(([0, 0, 0, 0], port));
        match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => {
                tracing::debug!("metrics listening on {}", addr);
                if let Err(e) = axum::serve(listener, router).await {
                    tracing::error!("metrics server error: {}", e);
                }
            }
            Err(e) => {
                tracing::error!("failed to bind metrics listener on {}: {}", addr, e);
            }
        }
    });
}
