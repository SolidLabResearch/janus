//! HTTP handlers for query management and result streaming.

use crate::{
    api::janus_api::{QueryHandle, QueryResult, ResultSource},
    http::{
        error::ApiError,
        types::{
            AppState, ListQueriesResponse, QueryDetailsResponse, QueryResultBroadcast,
            RegisterQueryRequest, RegisterQueryResponse, SuccessResponse,
        },
    },
    registry::query_registry::BaselineBootstrapMode,
};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    response::IntoResponse,
    Json,
};
use std::sync::Arc;
use tokio::sync::broadcast;

const RESULT_BROADCAST_CAPACITY: usize = 1024;

/// POST /api/queries - Register a new query
pub async fn register_query(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RegisterQueryRequest>,
) -> Result<Json<RegisterQueryResponse>, ApiError> {
    let baseline_mode = parse_baseline_mode(payload.baseline_mode.as_deref())?;
    let metadata = state.janus_api.register_query_with_baseline_mode(
        payload.query_id.clone(),
        &payload.janusql,
        baseline_mode,
    )?;

    Ok(Json(RegisterQueryResponse {
        query_id: metadata.query_id,
        query_text: metadata.query_text,
        registered_at: metadata.registered_at,
        message: "Query registered successfully".to_string(),
    }))
}

/// GET /api/queries - List all registered queries
pub async fn list_queries(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ListQueriesResponse>, ApiError> {
    let queries = state.registry.list_all();
    let total = queries.len();

    Ok(Json(ListQueriesResponse { queries, total }))
}

/// GET /api/queries/:id - Get query details
pub async fn get_query(
    State(state): State<Arc<AppState>>,
    Path(query_id): Path<String>,
) -> Result<Json<QueryDetailsResponse>, ApiError> {
    let metadata = state
        .registry
        .get(&query_id)
        .ok_or_else(|| ApiError::NotFound(format!("Query '{}' not found", query_id)))?;

    let is_running = state.janus_api.is_running(&query_id);

    Ok(Json(QueryDetailsResponse {
        query_id: metadata.query_id,
        query_text: metadata.query_text,
        baseline_mode: format!("{:?}", metadata.baseline_mode),
        registered_at: metadata.registered_at,
        execution_count: metadata.execution_count,
        is_running,
        status: metadata.status,
    }))
}

pub fn parse_baseline_mode(raw: Option<&str>) -> Result<BaselineBootstrapMode, ApiError> {
    match raw {
        None | Some("aggregate" | "AGGREGATE") => Ok(BaselineBootstrapMode::Aggregate),
        Some("last" | "LAST") => Ok(BaselineBootstrapMode::Last),
        Some(other) => Err(ApiError::BadRequest(format!(
            "Unsupported baseline_mode '{}'. Use 'aggregate' or 'last'",
            other
        ))),
    }
}

/// POST /api/queries/:id/start - Start executing a query
pub async fn start_query(
    State(state): State<Arc<AppState>>,
    Path(query_id): Path<String>,
) -> Result<Json<SuccessResponse>, ApiError> {
    let handle = state.janus_api.start_query(&query_id)?;
    let (sender, _) = broadcast::channel(RESULT_BROADCAST_CAPACITY);
    let sender_for_forwarder = sender.clone();

    std::thread::spawn(move || forward_query_results(handle, sender_for_forwarder));

    state
        .query_streams
        .lock()
        .unwrap()
        .insert(query_id.clone(), QueryResultBroadcast { sender });

    Ok(Json(SuccessResponse {
        message: format!("Query '{}' started successfully", query_id),
    }))
}

/// POST /api/queries/:id/stop - Stop a running query
pub async fn stop_query(
    State(state): State<Arc<AppState>>,
    Path(query_id): Path<String>,
) -> Result<Json<SuccessResponse>, ApiError> {
    state.janus_api.stop_query(&query_id)?;

    state.query_streams.lock().unwrap().remove(&query_id);

    Ok(Json(SuccessResponse {
        message: format!("Query '{}' stopped successfully", query_id),
    }))
}

/// DELETE /api/queries/:id - Unregister a query from the registry.
pub async fn delete_query(
    State(state): State<Arc<AppState>>,
    Path(query_id): Path<String>,
) -> Result<Json<SuccessResponse>, ApiError> {
    if state.janus_api.is_running(&query_id) {
        return Err(ApiError::BadRequest(format!(
            "Query '{}' is running. Stop it before deleting.",
            query_id
        )));
    }

    state
        .registry
        .unregister(&query_id)
        .map_err(|e| ApiError::NotFound(e.to_string()))?;
    state.query_streams.lock().unwrap().remove(&query_id);

    Ok(Json(SuccessResponse {
        message: format!("Query '{}' deleted successfully", query_id),
    }))
}

/// WS /api/queries/:id/results - Stream query results via WebSocket
pub async fn stream_results(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Path(query_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    // Check if query exists
    if state.registry.get(&query_id).is_none() {
        return Err(ApiError::NotFound(format!("Query '{}' not found", query_id)));
    }

    let sender = state
        .query_streams
        .lock()
        .unwrap()
        .get(&query_id)
        .map(|stream| stream.sender.clone())
        .ok_or_else(|| {
            ApiError::BadRequest(format!(
                "Query '{}' is not running. Start it before subscribing to results.",
                query_id
            ))
        })?;

    Ok(ws.on_upgrade(move |socket| handle_websocket(socket, sender.subscribe(), query_id)))
}

fn forward_query_results(handle: QueryHandle, sender: broadcast::Sender<QueryResult>) {
    while let Some(result) = handle.receive() {
        let _ = sender.send(result);
    }
}

async fn handle_websocket(
    mut socket: WebSocket,
    mut receiver: broadcast::Receiver<QueryResult>,
    query_id: String,
) {
    loop {
        let result = match receiver.recv().await {
            Ok(result) => result,
            Err(broadcast::error::RecvError::Closed) => break,
            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                let warning = serde_json::json!({
                    "query_id": query_id,
                    "type": "lagged",
                    "dropped_messages": skipped,
                });
                if socket.send(Message::Text(warning.to_string())).await.is_err() {
                    break;
                }
                continue;
            }
        };

        let json_result = serde_json::json!({
            "query_id": result.query_id,
            "timestamp": result.timestamp,
            "type": "result",
            "source": match result.source {
                ResultSource::Historical => "historical",
                ResultSource::Live => "live",
            },
            "bindings": result.bindings,
        });

        let message = Message::Text(json_result.to_string());

        if socket.send(message).await.is_err() {
            println!("WebSocket send error, client disconnected");
            break;
        } else {
            println!("Sent result to WebSocket for query {}", query_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_baseline_mode;
    use crate::registry::query_registry::BaselineBootstrapMode;

    #[test]
    fn test_parse_baseline_mode_defaults_to_aggregate() {
        assert_eq!(parse_baseline_mode(None).unwrap(), BaselineBootstrapMode::Aggregate);
    }

    #[test]
    fn test_parse_baseline_mode_accepts_last() {
        assert_eq!(parse_baseline_mode(Some("last")).unwrap(), BaselineBootstrapMode::Last);
    }
}
