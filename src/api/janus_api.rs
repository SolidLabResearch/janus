use crate::{
    parsing::janusql_parser::JanusQLParser,
    query,
    registry::query_registry::{QueryId, QueryMetadata, QueryRegistry},
    storage::segmented_storage::StreamingSegmentedStorage,
};
use std::{
    collections::HashMap,
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc, Mutex, RwLock,
    },
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

pub struct QueryHandle {
    pub query_id: QueryId,
    pub receiver: Receiver<QueryResult>,
}

impl std::error::Error for JanusApiError {}

impl QueryHandle {
    // Blocking receive method to get the next QueryResult
    pub fn receive(&self) -> Option<QueryResult> {
        self.receiver.recv().ok()
    }

    // Non-blocking try_receive method to get the next QueryResult if available
    pub fn try_receive(&self) -> Option<QueryResult> {
        self.receiver.try_recv().ok()
    }
}

#[allow(dead_code)]
struct RunningQuery {
    metadata: QueryMetadata,
    status: Arc<RwLock<ExecutionStatus>>,
    // Primary sender used to send the results to the main subscriber
    primary_sender: Sender<QueryResult>,
    // Additional senders for other subscribers (if any)
    subscribers: Vec<Sender<QueryResult>>,
    // thread handles for historical and live workers
    historical_handle: Option<std::thread::JoinHandle<()>>,
    live_handle: Option<std::thread::JoinHandle<()>>,
    // shutdown sender signals used to stop the workers
    shutdown_sender: Vec<Sender<()>>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
enum ExecutionStatus {
    Running,
    Stopped,
    Failed(String),
    Registered,
    Completed,
}

// Top-level API which coordinates the registry, the historical storage of data, and the live processing of data streams.
#[allow(dead_code)]
pub struct JanusApi {
    parser: JanusQLParser,
    registry: Arc<QueryRegistry>,
    storage: Arc<StreamingSegmentedStorage>,

    // The queries map
    running: Arc<Mutex<HashMap<QueryId, RunningQuery>>>,
}

impl JanusApi {
    pub fn new(
        parser: JanusQLParser,
        registry: Arc<QueryRegistry>,
        storage: Arc<StreamingSegmentedStorage>,
    ) -> Result<Self, JanusApiError> {
        Ok(JanusApi { parser, registry, storage, running: Arc::new(Mutex::new(HashMap::new())) })
    }

    // Register a JanusQL Query within the Query Registry.
    // It just stores the query without executing it.
    pub fn register_query(
        &self,
        query_id: QueryId,
        janusql: &str,
    ) -> Result<QueryMetadata, JanusApiError> {
        let parsed = self.parser.parse(janusql).map_err(|e| {
            JanusApiError::ParseError(format!("Failed to parse JanusQL query: {}", e))
        })?;
        let metadata = self
            .registry
            .register(query_id.clone(), janusql.to_string(), parsed)
            .map_err(|e| {
                JanusApiError::RegistryError(format!("Failed to register query: {}", e))
            })?;
        Ok(metadata)
    }

    // Start the execution of a registered JanusQL query.
    // This will spawn a thread for historical processing and another for live processing.
    // Returns a QueryHandle to receive results, then can be used to monitor the execution status.
    // pub fn start_query(&self, query_id: &QueryId) -> Result<QueryHandle, JanusApiError> {
    //     // Make sure that the query is registered already.
    //     let metadata = self.registry.get(&query_id).ok_or_else(|| JanusApiError::RegistryError("Query is not found".into()))?;

    //     // Do not start the query if it is already running.
    //     {
    //         let running_map = self.running.lock().unwrap();
    //         if running_map.contains_key(&query_id){
    //             return Err(JanusApiError::ExecutionError("The query is already running!".into()));
    //         }
    //     }

    //     let (result_tx, result_tx) = mpsc::channel()::<QueryResult>();

    //     let mut shutdown_senders = Vec::new();

    // }
}
