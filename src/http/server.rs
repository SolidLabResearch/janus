//! HTTP API Server for Janus
//!
//! Provides REST endpoints for query management and WebSocket streaming for results.
//! Also includes stream bus replay control for demo purposes.

use crate::{
    api::janus_api::{JanusApi, JanusApiError, QueryHandle, QueryResult, ResultSource},
    parsing::janusql_parser::JanusQLParser,
    parsing::rdf_parser,
    registry::query_registry::{QueryId, QueryRegistry},
    storage::segmented_storage::StreamingSegmentedStorage,
    stream_bus::{BrokerType, KafkaConfig, MqttConfig, StreamBus, StreamBusConfig},
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
use tokio::sync::mpsc;
use tower_http::cors::{Any, CorsLayer};

/// Request to register a new query
#[derive(Debug, Deserialize)]
pub struct RegisterQueryRequest {
    pub query_id: String,
    pub janusql: String,
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
    pub kafka_config: Option<KafkaConfigDto>,
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
pub struct KafkaConfigDto {
    pub bootstrap_servers: String,
    pub client_id: String,
    pub message_timeout_ms: String,
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
    pub query_handles: Arc<Mutex<HashMap<QueryId, Arc<Mutex<QueryHandle>>>>>,
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
    let state = Arc::new(AppState {
        janus_api,
        registry,
        storage,
        replay_state: Arc::new(Mutex::new(ReplayState::default())),
        query_handles: Arc::new(Mutex::new(HashMap::new())),
    });

    // Configure CORS
    let cors = CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any);

    Router::new()
        .route("/api/queries", post(register_query))
        .route("/api/queries", get(list_queries))
        .route("/api/queries/:id", get(get_query))
        .route("/api/queries/:id", delete(stop_query))
        .route("/api/queries/:id/start", post(start_query))
        .route("/api/queries/:id/results", get(stream_results))
        .route("/api/replay/start", post(start_replay))
        .route("/api/replay/stop", post(stop_replay))
        .route("/api/replay/status", get(replay_status))
        .route("/health", get(health_check))
        .layer(cors)
        .with_state(state)
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
    let metadata = state.janus_api.register_query(payload.query_id.clone(), &payload.janusql)?;

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
    let status = if is_running {
        state
            .janus_api
            .get_query_status(&query_id)
            .map(|s| format!("{:?}", s))
            .unwrap_or_else(|| "Unknown".to_string())
    } else {
        "Registered".to_string()
    };

    Ok(Json(QueryDetailsResponse {
        query_id: metadata.query_id,
        query_text: metadata.query_text,
        registered_at: metadata.registered_at,
        execution_count: metadata.execution_count,
        is_running,
        status,
    }))
}

/// POST /api/queries/:id/start - Start executing a query
async fn start_query(
    State(state): State<Arc<AppState>>,
    Path(query_id): Path<String>,
) -> Result<Json<SuccessResponse>, ApiError> {
    let handle = state.janus_api.start_query(&query_id)?;

    // Store the handle for WebSocket streaming
    state
        .query_handles
        .lock()
        .unwrap()
        .insert(query_id.clone(), Arc::new(Mutex::new(handle)));

    Ok(Json(SuccessResponse {
        message: format!("Query '{}' started successfully", query_id),
    }))
}

/// DELETE /api/queries/:id - Stop a running query
async fn stop_query(
    State(state): State<Arc<AppState>>,
    Path(query_id): Path<String>,
) -> Result<Json<SuccessResponse>, ApiError> {
    state.janus_api.stop_query(&query_id)?;

    // Remove the handle
    state.query_handles.lock().unwrap().remove(&query_id);

    Ok(Json(SuccessResponse {
        message: format!("Query '{}' stopped successfully", query_id),
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

    Ok(ws.on_upgrade(move |socket| handle_websocket(socket, state, query_id)))
}

async fn handle_websocket(mut socket: WebSocket, state: Arc<AppState>, query_id: String) {
    // Create a channel for results
    let (tx, mut rx) = mpsc::unbounded_channel::<QueryResult>();

    // Spawn a task to receive results from the query handle
    let handles = state.query_handles.clone();
    let query_id_clone = query_id.clone();

    tokio::spawn(async move {
        loop {
            // Try to get the query handle
            let handle_opt = {
                let handles_lock = handles.lock().unwrap();
                handles_lock.get(&query_id_clone).cloned()
            };

            if let Some(handle_arc) = handle_opt {
                let handle = handle_arc.lock().unwrap();

                // Non-blocking receive
                if let Some(result) = handle.try_receive() {
                    if tx.send(result).is_err() {
                        break;
                    }
                }
            } else {
                // Query handle not found, wait a bit and retry
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }

            // Small delay to prevent busy waiting
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
    });

    // Send results to WebSocket
    while let Some(result) = rx.recv().await {
        let json_result = serde_json::json!({
            "query_id": result.query_id,
            "timestamp": result.timestamp,
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
        "kafka" => BrokerType::Kafka,
        "mqtt" => BrokerType::Mqtt,
        "none" => BrokerType::None,
        _ => {
            return Err(ApiError::BadRequest(format!(
                "Invalid broker type: {}. Use 'kafka', 'mqtt', or 'none'",
                payload.broker_type
            )))
        }
    };

    // Convert configs
    let kafka_config = payload.kafka_config.map(|cfg| KafkaConfig {
        bootstrap_servers: cfg.bootstrap_servers,
        client_id: cfg.client_id,
        message_timeout_ms: cfg.message_timeout_ms,
    });

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
        kafka_config,
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
    println!("  DELETE /api/queries/:id          - Stop a running query");
    println!("  WS     /api/queries/:id/results  - Stream query results (WebSocket)");
    println!("  POST   /api/replay/start         - Start stream bus replay");
    println!("  POST   /api/replay/stop          - Stop stream bus replay");
    println!("  GET    /api/replay/status        - Get replay status");
    println!("  GET    /health                   - Health check");
    println!();

    axum::serve(listener, app).await?;

    Ok(())
}
