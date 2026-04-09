//! HTTP API Server for Janus
//!
//! Provides REST endpoints for query management and WebSocket streaming for results.
//! Also includes stream bus replay control for demo purposes.

use crate::{
    api::janus_api::{JanusApi, JanusApiError, QueryHandle, QueryResult, ResultSource},
    registry::query_registry::{BaselineBootstrapMode, QueryId, QueryRegistry},
    storage::segmented_storage::StreamingSegmentedStorage,
    stream_bus::{BrokerType, MqttConfig, StreamBus, StreamBusConfig},
};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};
use tokio::sync::broadcast;
use tower_http::cors::{Any, CorsLayer};

const RESULT_BROADCAST_CAPACITY: usize = 1024;

/// Request to register a new query
#[derive(Debug, Deserialize)]
pub struct RegisterQueryRequest {
    pub query_id: String,
    pub janusql: String,
    pub baseline_mode: Option<String>,
}

/// Response after registering a query
#[derive(Debug, Serialize)]
pub struct RegisterQueryResponse {
    pub query_id: String,
    pub query_text: String,
    pub registered_at: u64,
    pub message: String,
}

/// Response for query details
#[derive(Debug, Serialize)]
pub struct QueryDetailsResponse {
    pub query_id: String,
    pub query_text: String,
    pub baseline_mode: String,
    pub registered_at: u64,
    pub execution_count: u64,
    pub is_running: bool,
    pub status: String,
}

/// Response for listing queries
#[derive(Debug, Serialize)]
pub struct ListQueriesResponse {
    pub queries: Vec<String>,
    pub total: usize,
}

/// Generic success response
#[derive(Debug, Serialize)]
pub struct SuccessResponse {
    pub message: String,
}

/// Error response
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

/// Request to start stream bus replay
#[derive(Debug, Deserialize)]
pub struct StartReplayRequest {
    pub input_file: String,
    #[serde(default = "default_broker_type")]
    pub broker_type: String,
    #[serde(default = "default_topics")]
    pub topics: Vec<String>,
    #[serde(default = "default_rate")]
    pub rate_of_publishing: u64,
    #[serde(default)]
    pub loop_file: bool,
    #[serde(default = "default_true")]
    pub add_timestamps: bool,
    pub mqtt_config: Option<MqttConfigDto>,
}

fn default_broker_type() -> String {
    "none".to_string()
}

fn default_topics() -> Vec<String> {
    vec!["janus".to_string()]
}

fn default_rate() -> u64 {
    1000
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
pub struct MqttConfigDto {
    pub host: String,
    pub port: u16,
    pub client_id: String,
    pub keep_alive_secs: u64,
}

/// Response for replay status
#[derive(Debug, Serialize)]
pub struct ReplayStatusResponse {
    pub is_running: bool,
    pub events_read: u64,
    pub events_published: u64,
    pub events_stored: u64,
    pub publish_errors: u64,
    pub storage_errors: u64,
    pub events_per_second: f64,
    pub elapsed_seconds: f64,
}

/// Shared application state
pub struct AppState {
    pub janus_api: Arc<JanusApi>,
    pub registry: Arc<QueryRegistry>,
    pub storage: Arc<StreamingSegmentedStorage>,
    pub replay_state: Arc<Mutex<ReplayState>>,
    pub query_streams: Arc<Mutex<HashMap<QueryId, QueryResultBroadcast>>>,
}

#[derive(Clone)]
pub struct QueryResultBroadcast {
    pub sender: broadcast::Sender<QueryResult>,
}

pub struct ReplayState {
    pub is_running: bool,
    pub start_time: Option<Instant>,
    pub input_file: Option<String>,
    pub stream_bus: Option<Arc<StreamBus>>,
    pub events_read: Arc<AtomicU64>,
    pub events_published: Arc<AtomicU64>,
    pub events_stored: Arc<AtomicU64>,
    pub publish_errors: Arc<AtomicU64>,
    pub storage_errors: Arc<AtomicU64>,
}

impl Default for ReplayState {
    fn default() -> Self {
        Self {
            is_running: false,
            start_time: None,
            input_file: None,
            stream_bus: None,
            events_read: Arc::new(AtomicU64::new(0)),
            events_published: Arc::new(AtomicU64::new(0)),
            events_stored: Arc::new(AtomicU64::new(0)),
            publish_errors: Arc::new(AtomicU64::new(0)),
            storage_errors: Arc::new(AtomicU64::new(0)),
        }
    }
}

/// Custom error type for API errors
#[derive(Debug)]
pub enum ApiError {
    JanusError(JanusApiError),
    NotFound(String),
    BadRequest(String),
    InternalError(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::JanusError(e) => (StatusCode::BAD_REQUEST, e.to_string()),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            ApiError::InternalError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        let body = Json(ErrorResponse { error: message });
        (status, body).into_response()
    }
}

impl From<JanusApiError> for ApiError {
    fn from(err: JanusApiError) -> Self {
        ApiError::JanusError(err)
    }
}

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
        replay_state: Arc::new(Mutex::new(ReplayState::default())),
        query_streams: Arc::new(Mutex::new(HashMap::new())),
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
        .route("/health", get(health_check))
        .layer(cors)
        .with_state(Arc::clone(&state));

    (router, state)
}

/// Health check endpoint
async fn health_check() -> impl IntoResponse {
    Json(SuccessResponse { message: "Janus HTTP API is running".to_string() })
}

/// POST /api/queries - Register a new query
async fn register_query(
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
async fn list_queries(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ListQueriesResponse>, ApiError> {
    let queries = state.registry.list_all();
    let total = queries.len();

    Ok(Json(ListQueriesResponse { queries, total }))
}

/// GET /api/queries/:id - Get query details
async fn get_query(
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

fn parse_baseline_mode(raw: Option<&str>) -> Result<BaselineBootstrapMode, ApiError> {
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
async fn start_query(
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
async fn stop_query(
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
async fn delete_query(
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
async fn stream_results(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Path(query_id): Path<String>,
) -> Result<Response, ApiError> {
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

/// POST /api/replay/start - Start stream bus replay
async fn start_replay(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<StartReplayRequest>,
) -> Result<Json<SuccessResponse>, ApiError> {
    let mut replay_state = state.replay_state.lock().unwrap();

    if replay_state.is_running {
        return Err(ApiError::BadRequest("Replay is already running".to_string()));
    }

    // Parse broker type
    let broker_type = match payload.broker_type.to_lowercase().as_str() {
        "mqtt" => BrokerType::Mqtt,
        "none" => BrokerType::None,
        _ => {
            return Err(ApiError::BadRequest(format!(
                "Invalid broker type: {}. Use 'mqtt' or 'none'",
                payload.broker_type
            )))
        }
    };

    // Convert configs
    let mqtt_config = payload.mqtt_config.map(|cfg| MqttConfig {
        host: cfg.host,
        port: cfg.port,
        client_id: cfg.client_id,
        keep_alive_secs: cfg.keep_alive_secs,
    });

    let bus_config = StreamBusConfig {
        input_file: payload.input_file.clone(),
        broker_type,
        topics: payload.topics,
        rate_of_publishing: payload.rate_of_publishing,
        loop_file: payload.loop_file,
        add_timestamps: payload.add_timestamps,
        mqtt_config,
    };

    let storage = Arc::clone(&state.storage);
    let input_file_clone = payload.input_file.clone();

    // Create StreamBus and store it in state
    let stream_bus = Arc::new(StreamBus::new(bus_config, storage));
    let stream_bus_clone = Arc::clone(&stream_bus);

    // Clone metric counters from StreamBus
    let events_read = Arc::clone(&stream_bus.events_read);
    let events_published = Arc::clone(&stream_bus.events_published);
    let events_stored = Arc::clone(&stream_bus.events_stored);
    let publish_errors = Arc::clone(&stream_bus.publish_errors);
    let storage_errors = Arc::clone(&stream_bus.storage_errors);

    let replay_state_clone = Arc::clone(&state.replay_state);

    // Spawn replay in a blocking thread to avoid runtime conflict
    std::thread::spawn(move || {
        if let Err(e) = stream_bus_clone.start() {
            eprintln!("Stream bus replay error: {}", e);
        }

        // Reset running state when finished
        if let Ok(mut rs) = replay_state_clone.lock() {
            rs.is_running = false;
            rs.start_time = None;
            println!("Stream bus replay finished");
        }
    });

    // Safely drop the old stream_bus if it exists, to avoid dropping a Runtime in async context
    let old_stream_bus = replay_state.stream_bus.take();
    if let Some(bus) = old_stream_bus {
        tokio::task::spawn_blocking(move || {
            drop(bus);
        });
    }

    replay_state.is_running = true;
    replay_state.start_time = Some(Instant::now());
    replay_state.input_file = Some(input_file_clone);
    replay_state.stream_bus = Some(stream_bus);
    replay_state.events_read = events_read;
    replay_state.events_published = events_published;
    replay_state.events_stored = events_stored;
    replay_state.publish_errors = publish_errors;
    replay_state.storage_errors = storage_errors;

    Ok(Json(SuccessResponse {
        message: format!("Stream bus replay started with file: {}", payload.input_file),
    }))
}

/// POST /api/replay/stop - Stop stream bus replay
async fn stop_replay(
    State(state): State<Arc<AppState>>,
) -> Result<Json<SuccessResponse>, ApiError> {
    let mut replay_state = state.replay_state.lock().unwrap();

    if !replay_state.is_running {
        return Err(ApiError::BadRequest("Replay is not running".to_string()));
    }

    // Stop the stream bus if it exists
    if let Some(stream_bus) = &replay_state.stream_bus {
        stream_bus.stop();
    }

    replay_state.is_running = false;
    replay_state.start_time = None;
    replay_state.input_file = None;
    replay_state.stream_bus = None;

    Ok(Json(SuccessResponse { message: "Stream bus replay stopped".to_string() }))
}

/// GET /api/replay/status - Get replay status
async fn replay_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ReplayStatusResponse>, ApiError> {
    let replay_state = state.replay_state.lock().unwrap();

    let elapsed_seconds = if replay_state.is_running {
        replay_state.start_time.map_or(0.0, |t| t.elapsed().as_secs_f64())
    } else {
        0.0
    };

    let events_read = replay_state.events_read.load(Ordering::Relaxed);
    let events_published = replay_state.events_published.load(Ordering::Relaxed);
    let events_stored = replay_state.events_stored.load(Ordering::Relaxed);
    let publish_errors = replay_state.publish_errors.load(Ordering::Relaxed);
    let storage_errors = replay_state.storage_errors.load(Ordering::Relaxed);

    let events_per_second = if elapsed_seconds > 0.0 {
        events_read as f64 / elapsed_seconds
    } else {
        0.0
    };

    Ok(Json(ReplayStatusResponse {
        is_running: replay_state.is_running,
        events_read,
        events_published,
        events_stored,
        publish_errors,
        storage_errors,
        events_per_second,
        elapsed_seconds,
    }))
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
    println!("  GET    /health                   - Health check");
    println!();

    axum::serve(listener, app).await?;

    Ok(())
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
