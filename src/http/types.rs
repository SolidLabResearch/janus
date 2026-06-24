//! Shared state and data transfer objects for the HTTP API server.

use crate::{
    api::janus_api::{JanusApi, QueryResult},
    registry::query_registry::{QueryId, QueryRegistry},
    storage::segmented_storage::StreamingSegmentedStorage,
    stream_bus::{MqttConfig, StreamBus},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{
        atomic::AtomicU64,
        Arc, Mutex,
    },
    time::Instant,
};
use tokio::sync::broadcast;

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

/// Response for service health.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub message: String,
    pub storage_status: String,
    pub storage_error: Option<String>,
}

/// Detailed storage status for ops surfaces.
#[derive(Debug, Serialize)]
pub struct StorageStatusResponse {
    pub status: String,
    pub background_flush_error: Option<String>,
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

pub fn default_broker_type() -> String {
    "none".to_string()
}

pub fn default_topics() -> Vec<String> {
    vec!["janus".to_string()]
}

pub fn default_rate() -> u64 {
    1000
}

pub fn default_true() -> bool {
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
#[derive(Debug, Serialize, Clone)]
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

/// Query lifecycle status summary for ops surfaces.
#[derive(Debug, Serialize)]
pub struct QueryOpsStatusResponse {
    pub total_registered_queries: usize,
    pub active_runtime_queries: usize,
    pub registered_queries: usize,
    pub warming_baseline_queries: usize,
    pub running_queries: usize,
    pub stopped_queries: usize,
    pub failed_queries: usize,
}

/// Rich operational status response.
#[derive(Debug, Serialize)]
pub struct OpsStatusResponse {
    pub status: String,
    pub message: String,
    pub storage: StorageStatusResponse,
    pub replay: ReplayStatusResponse,
    pub queries: QueryOpsStatusResponse,
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
