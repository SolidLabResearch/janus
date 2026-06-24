//! HTTP handlers for health checks and system operational status.

use crate::{
    http::{
        handlers::replay::replay_status_snapshot,
        types::{
            AppState, HealthResponse, OpsStatusResponse, QueryOpsStatusResponse,
            StorageStatusResponse,
        },
    },
    storage::segmented_storage::StreamingSegmentedStorage,
};
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use std::sync::Arc;

/// Health check endpoint
pub async fn health_check(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let storage = storage_status(&state.storage);

    if let Some(storage_error) = storage.background_flush_error.clone() {
        let response = HealthResponse {
            status: "degraded".to_string(),
            message: "Janus HTTP API is running with storage errors".to_string(),
            storage_status: storage.status,
            storage_error: Some(storage_error),
        };
        return (StatusCode::SERVICE_UNAVAILABLE, Json(response)).into_response();
    }

    (
        StatusCode::OK,
        Json(HealthResponse {
            status: "ok".to_string(),
            message: "Janus HTTP API is running".to_string(),
            storage_status: storage.status,
            storage_error: None,
        }),
    )
        .into_response()
}

/// Operational status endpoint.
pub async fn ops_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let storage = storage_status(&state.storage);
    let replay = replay_status_snapshot(&state.replay_state.lock().unwrap());
    let queries = query_ops_status(&state);

    let (status, message) = if storage.background_flush_error.is_some() {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Janus HTTP API is running with degraded storage".to_string(),
        )
    } else {
        (StatusCode::OK, "Janus HTTP API is running".to_string())
    };

    (
        status,
        Json(OpsStatusResponse {
            status: if status == StatusCode::OK {
                "ok".to_string()
            } else {
                "degraded".to_string()
            },
            message,
            storage,
            replay,
            queries,
        }),
    )
        .into_response()
}

pub fn storage_status(storage: &StreamingSegmentedStorage) -> StorageStatusResponse {
    StorageStatusResponse {
        status: if storage.background_flush_error().is_some() {
            "error".to_string()
        } else {
            "ok".to_string()
        },
        background_flush_error: storage.background_flush_error(),
    }
}

pub fn query_ops_status(state: &Arc<AppState>) -> QueryOpsStatusResponse {
    let query_ids = state.registry.list_all();
    let mut registered_queries = 0;
    let mut warming_baseline_queries = 0;
    let mut running_queries = 0;
    let mut stopped_queries = 0;
    let mut failed_queries = 0;

    for query_id in &query_ids {
        if let Some(metadata) = state.registry.get(query_id) {
            match metadata.status.as_str() {
                "Registered" => registered_queries += 1,
                "WarmingBaseline" => warming_baseline_queries += 1,
                "Running" => running_queries += 1,
                "Stopped" => stopped_queries += 1,
                status if status.starts_with("Failed") => failed_queries += 1,
                _ => {}
            }
        }
    }

    let active_runtime_queries =
        query_ids.iter().filter(|query_id| state.janus_api.is_running(query_id)).count();

    QueryOpsStatusResponse {
        total_registered_queries: query_ids.len(),
        active_runtime_queries,
        registered_queries,
        warming_baseline_queries,
        running_queries,
        stopped_queries,
        failed_queries,
    }
}
