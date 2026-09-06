mod db;
mod routes;
mod runner;
mod stats;

use axum::{
    routing::{get, post},
    Router,
};
use routes::AppState;
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "api=info,simulation=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let state = AppState::new();

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/health", get(routes::get_health))
        .route("/sim/batch", post(routes::post_batch))
        .route("/sim/batch/status", get(routes::get_batch_status))
        .route("/sim/batch/events", get(routes::get_batch_events))
        .route("/sim/batch/results", get(routes::get_batch_results))
        .route("/sim/batch/stats", get(routes::get_batch_stats))
        .route("/sim/batch/hypothesis", get(routes::get_batch_hypothesis))
        .route("/sim/batch/export.parquet", get(routes::get_batch_export))
        .route("/sim/ais-paths", get(routes::get_ais_paths))
        .route("/sim/runs", get(routes::get_runs))
        .route("/sim/runs/{run_id}/log", get(routes::get_run_log))
        .route("/sim/runs/{run_id}/manifest", get(routes::get_run_manifest))
        .with_state(state)
        .layer(cors);

    let addr = "0.0.0.0:3000";
    tracing::info!("listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
