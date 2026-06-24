use crate::{
    registry::{
        baseline_registry::BaselineRegistry,
        query_registry::{QueryId, QueryMetadata},
    },
    stream::mqtt_subscriber::MqttSubscriber,
};
use std::{
    collections::HashMap,
    sync::{
        mpsc::{Receiver, Sender},
        Arc, RwLock,
    },
    thread::JoinHandle,
};

/// The Query Result created from a query execution of a JanusQL query.
#[derive(Debug, Clone)]
pub struct QueryResult {
    pub query_id: QueryId,
    pub timestamp: u64,
    pub source: ResultSource,
    pub bindings: Vec<HashMap<String, String>>,
}

/// Enum representing the source of the query result.
#[derive(Debug, Clone)]
pub enum ResultSource {
    Historical,
    Live,
}

/// Enum representing the errors that might occur during the query execution and just general API operations.
#[derive(Debug)]
pub enum JanusApiError {
    ParseError(String),
    ExecutionError(String),
    RegistryError(String),
    StorageError(String),
    LiveProcessingError(String),
}

impl std::fmt::Display for JanusApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JanusApiError::ParseError(msg) => write!(f, "Parse Error: {}", msg),
            JanusApiError::ExecutionError(msg) => write!(f, "Execution Error: {}", msg),
            JanusApiError::RegistryError(msg) => write!(f, "Registry Error: {}", msg),
            JanusApiError::StorageError(msg) => write!(f, "Storage Error: {}", msg),
            JanusApiError::LiveProcessingError(msg) => write!(f, "Live Processing Error: {}", msg),
        }
    }
}

impl std::error::Error for JanusApiError {}

pub struct QueryHandle {
    pub query_id: QueryId,
    pub receiver: Receiver<QueryResult>,
}

impl QueryHandle {
    /// Blocking receive method to get the next QueryResult
    pub fn receive(&self) -> Option<QueryResult> {
        self.receiver.recv().ok()
    }

    /// Non-blocking try_receive method to get the next QueryResult if available
    pub fn try_receive(&self) -> Option<QueryResult> {
        self.receiver.try_recv().ok()
    }
}

#[allow(dead_code)]
pub(crate) struct RunningQuery {
    pub(crate) metadata: QueryMetadata,
    pub(crate) status: Arc<RwLock<ExecutionStatus>>,
    /// Query-defined baselines are evaluated at startup and stored here for
    /// inspection/debugging as the latest SELECT-result snapshot rows.
    pub(crate) query_defined_baselines: Arc<RwLock<HashMap<String, Vec<HashMap<String, String>>>>>,
    pub(crate) baseline_registry: Arc<BaselineRegistry>,
    /// Primary sender used to send the results to the main subscriber
    pub(crate) primary_sender: Sender<QueryResult>,
    /// Additional senders for other subscribers (if any)
    pub(crate) subscribers: Vec<Sender<QueryResult>>,
    /// thread handles for historical and live workers
    pub(crate) historical_handles: Vec<JoinHandle<()>>,
    pub(crate) baseline_handle: Option<JoinHandle<()>>,
    pub(crate) live_handle: Option<JoinHandle<()>>,
    pub(crate) mqtt_subscriber_handles: Vec<JoinHandle<()>>,
    /// shutdown sender signals used to stop the workers
    pub(crate) shutdown_senders: Vec<Sender<()>>,
    /// MQTT subscriber instances (for stopping)
    pub(crate) mqtt_subscribers: Vec<Arc<MqttSubscriber>>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionStatus {
    WarmingBaseline,
    Running,
    Stopped,
    Failed(String),
    Registered,
    Completed,
}
