//! Axum HTTP/WebSocket API server with JWT auth, rate limiting, and Prometheus metrics.

pub mod auth;
pub mod middleware;
pub mod routes;
pub mod state;
pub mod ws;

pub use state::AppState;

use axum::{
    Router,
    routing::{get, post},
};
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::{
    cors::{Any, CorsLayer},
    set_header::SetResponseHeaderLayer,
    trace::TraceLayer,
};
use axum::http::{HeaderValue, HeaderName};
use tracing::info;

/// Build the Axum application router.
pub fn build_router(state: Arc<AppState>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([axum::http::Method::GET, axum::http::Method::POST])
        .allow_headers(Any);

    Router::new()
        // ── Health & info ──────────────────────────────────────────────
        .route("/health",         get(routes::health::health))
        .route("/api/v1/info",    get(routes::info::info))
        .route("/metrics",        get(routes::metrics::prometheus_metrics))
        // ── Sensing ────────────────────────────────────────────────────
        .route("/api/v1/sensing/latest", get(routes::sensing::latest_frame))
        .route("/api/v1/room/field",     get(routes::sensing::room_field))
        // ── Pose ───────────────────────────────────────────────────────
        .route("/api/v1/pose/current",   get(routes::pose::current_pose))
        // ── Vitals ─────────────────────────────────────────────────────
        .route("/api/v1/vital-signs",    get(routes::vitals::vital_signs))
        // ── WebSocket ──────────────────────────────────────────────────
        .route("/ws/sensing",            get(ws::sensing_ws_handler))
        // ── Middleware stack ───────────────────────────────────────────
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .layer(SetResponseHeaderLayer::if_not_present(
            HeaderName::from_static("x-content-type-options"),
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            HeaderName::from_static("x-frame-options"),
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            HeaderName::from_static("x-ruview-version"),
            HeaderValue::from_static("0.3.0"),
        ))
        .with_state(state)
}

/// Start the HTTP server on the given address.
pub async fn serve(router: Router, addr: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
    info!(%addr, "RuView API server starting");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router).await?;
    Ok(())
}
