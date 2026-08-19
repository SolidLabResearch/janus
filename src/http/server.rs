//! HTTP API Server for Janus
//!
//! Provides REST endpoints for query management and WebSocket streaming for results.
//! Also includes stream bus replay control for demo purposes.

use crate::{
    api::janus_api::JanusApi, registry::query_registry::QueryRegistry,
    storage::segmented_storage::StreamingSegmentedStorage,
};
use axum::{
    routing::{delete, get, post},
    Router,
};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

pub use crate::http::error::{ApiError, ErrorResponse};
pub use crate::http::types::*;

use crate::http::handlers::{
    delete_query, get_query, health_check, list_queries, ops_status, register_query, replay_status,
    start_query, start_replay, stop_query, stop_replay, stream_results,
};

/// Create the HTTP server with all routes
pub fn create_server(
    janus_api: Arc<JanusApi>,
    registry: Arc<QueryRegistry>,
    storage: Arc<StreamingSegmentedStorage>,
) -> Router {
    create_server_with_state(janus_api, registry, storage).0
}

/// Create the HTTP server and return the shared state for testing/integration.
pub fn create_server_with_state(
    janus_api: Arc<JanusApi>,
    registry: Arc<QueryRegistry>,
    storage: Arc<StreamingSegmentedStorage>,
) -> (Router, Arc<AppState>) {
    let state = Arc::new(AppState {
        janus_api,
        registry,
        storage,
        replay_state: Arc::new(std::sync::Mutex::new(ReplayState::default())),
        query_streams: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
    });

    // Configure CORS
    let cors = CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any);

    let router = Router::new()
        .route("/api/queries", post(register_query))
        .route("/api/queries", get(list_queries))
        .route("/api/queries/:id", get(get_query))
        .route("/api/queries/:id", delete(delete_query))
        .route("/api/queries/:id/start", post(start_query))
        .route("/api/queries/:id/stop", post(stop_query))
        .route("/api/queries/:id/results", get(stream_results))
        .route("/api/replay/start", post(start_replay))
        .route("/api/replay/stop", post(stop_replay))
        .route("/api/replay/status", get(replay_status))
        .route("/ops/status", get(ops_status))
        .route("/health", get(health_check))
        .layer(cors)
        .with_state(Arc::clone(&state));

    (router, state)
}

/// Start the HTTP server on the specified address
pub async fn start_server(
    addr: &str,
    janus_api: Arc<JanusApi>,
    registry: Arc<QueryRegistry>,
    storage: Arc<StreamingSegmentedStorage>,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = create_server(janus_api, registry, storage);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("Janus HTTP API server listening on http://{}", addr);
    println!();
    println!("Available endpoints:");
    println!("  POST   /api/queries              - Register a new query");
    println!("  GET    /api/queries              - List all registered queries");
    println!("  GET    /api/queries/:id          - Get query details");
    println!("  POST   /api/queries/:id/start    - Start executing a query");
    println!("  POST   /api/queries/:id/stop     - Stop a running query");
    println!("  DELETE /api/queries/:id          - Delete a stopped query");
    println!("  WS     /api/queries/:id/results  - Stream query results (WebSocket)");
    println!("  POST   /api/replay/start         - Start stream bus replay");
    println!("  POST   /api/replay/stop          - Stop stream bus replay");
    println!("  GET    /api/replay/status        - Get replay status");
    println!("  GET    /ops/status               - Detailed operational status");
    println!("  GET    /health                   - Health check");
    println!();

    axum::serve(listener, app).await?;

    Ok(())
}
