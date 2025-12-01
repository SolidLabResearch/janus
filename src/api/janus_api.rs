use crate::{
    execution::{HistoricalExecutor, ResultConverter},
    parsing::janusql_parser::{JanusQLParser, WindowType},
    querying::oxigraph_adapter::OxigraphAdapter,
    registry::query_registry::{QueryId, QueryMetadata, QueryRegistry},
    storage::segmented_storage::StreamingSegmentedStorage,
    stream::{
        live_stream_processing::LiveStreamProcessing,
        mqtt_subscriber::{MqttSubscriber, MqttSubscriberConfig},
    },
};
use std::{
    collections::HashMap,
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc, Mutex, RwLock,
    },
    thread,
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
    historical_handles: Vec<thread::JoinHandle<()>>,
    live_handle: Option<thread::JoinHandle<()>>,
    mqtt_subscriber_handle: Option<thread::JoinHandle<()>>,
    // shutdown sender signals used to stop the workers
    shutdown_senders: Vec<Sender<()>>,
    // MQTT subscriber instances (for stopping)
    mqtt_subscribers: Vec<Arc<MqttSubscriber>>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionStatus {
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

    /// Start the execution of a registered JanusQL query.
    ///
    /// This spawns threads for both historical and live processing:
    /// - Historical threads: One per historical window, processes past data
    /// - Live thread: One thread processing RSP-QL query for all live windows
    ///
    /// Both historical and live results are sent to the same channel, allowing
    /// users to receive a unified stream of results.
    ///
    /// # Arguments
    ///
    /// * `query_id` - The ID of the previously registered query
    ///
    /// # Returns
    ///
    /// A `QueryHandle` that can be used to receive results via `receive()` or `try_receive()`
    ///
    /// # Example
    ///
    /// ```ignore
    /// let handle = api.start_query(&"my_query".into())?;
    ///
    /// while let Some(result) = handle.receive() {
    ///     match result.source {
    ///         ResultSource::Historical => println!("Historical: {:?}", result.bindings),
    ///         ResultSource::Live => println!("Live: {:?}", result.bindings),
    ///     }
    /// }
    /// ```
    pub fn start_query(&self, query_id: &QueryId) -> Result<QueryHandle, JanusApiError> {
        // 1. Make sure the query is registered
        let metadata = self.registry.get(query_id).ok_or_else(|| {
            JanusApiError::RegistryError(format!("Query '{}' not found in registry", query_id))
        })?;

        // 2. Check if query is already running
        {
            let running_map = self.running.lock().unwrap();
            if running_map.contains_key(query_id) {
                return Err(JanusApiError::ExecutionError(format!(
                    "Query '{}' is already running",
                    query_id
                )));
            }
        }

        // 3. Create unified result channel
        let (result_tx, result_rx) = mpsc::channel::<QueryResult>();

        let parsed = &metadata.parsed;
        let mut historical_handles = Vec::new();
        let mut shutdown_senders = Vec::new();

        // 4. Spawn historical worker threads (one per historical window)
        for (i, window) in parsed.historical_windows.iter().enumerate() {
            // Get corresponding SPARQL query
            let sparql_query = parsed
                .sparql_queries
                .get(i)
                .ok_or_else(|| {
                    JanusApiError::ExecutionError(format!(
                        "Missing SPARQL query for historical window {}",
                        i
                    ))
                })?
                .clone();

            let tx = result_tx.clone();
            let storage = Arc::clone(&self.storage);
            let window_clone = window.clone();
            let query_id_clone = query_id.clone();
            let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();

            let handle = thread::spawn(move || {
                let executor = HistoricalExecutor::new(storage, OxigraphAdapter::new());
                let converter = ResultConverter::new(query_id_clone);

                match window_clone.window_type {
                    WindowType::HistoricalFixed => {
                        // Execute once for fixed window
                        match executor.execute_fixed_window(&window_clone, &sparql_query) {
                            Ok(bindings) => {
                                let timestamp = window_clone.end.unwrap_or(0);
                                let result =
                                    converter.from_historical_bindings(bindings, timestamp);
                                let _ = tx.send(result);
                            }
                            Err(e) => {
                                eprintln!("Historical fixed window error: {}", e);
                            }
                        }
                    }
                    WindowType::HistoricalSliding => {
                        // Execute for each sliding window
                        for window_result in
                            executor.execute_sliding_windows(&window_clone, &sparql_query)
                        {
                            // Check for shutdown signal
                            if shutdown_rx.try_recv().is_ok() {
                                break;
                            }

                            match window_result {
                                Ok(bindings) => {
                                    let timestamp = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_millis()
                                        as u64;
                                    let result =
                                        converter.from_historical_bindings(bindings, timestamp);
                                    let _ = tx.send(result);
                                }
                                Err(e) => {
                                    eprintln!("Historical sliding window error: {}", e);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            });

            historical_handles.push(handle);
            shutdown_senders.push(shutdown_tx);
        }

        // 5. Spawn live worker thread and MQTT subscribers (if there are live windows)
        let mut mqtt_subscribers = Vec::new();
        let mut mqtt_subscriber_handle = None;

        let live_handle = if !parsed.live_windows.is_empty() && !parsed.rspql_query.is_empty() {
            let tx = result_tx.clone();
            let rspql = parsed.rspql_query.clone();
            let query_id_clone = query_id.clone();
            let live_windows = parsed.live_windows.clone();
            let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();

            // Create LiveStreamProcessing wrapped in Arc<Mutex<>> for sharing with MQTT subscriber
            let live_processor = match LiveStreamProcessing::new(rspql) {
                Ok(processor) => Arc::new(Mutex::new(processor)),
                Err(e) => {
                    eprintln!("Failed to create LiveStreamProcessing: {}", e);
                    return Err(JanusApiError::LiveProcessingError(format!(
                        "Failed to create live processor: {}",
                        e
                    )));
                }
            };

            // Register all live streams
            {
                let mut processor = live_processor.lock().unwrap();
                for window in &live_windows {
                    if let Err(e) = processor.register_stream(&window.stream_name) {
                        eprintln!("Failed to register stream '{}': {}", window.stream_name, e);
                    }
                }

                // Start processing
                if let Err(e) = processor.start_processing() {
                    eprintln!("Failed to start live processing: {}", e);
                    return Err(JanusApiError::LiveProcessingError(format!(
                        "Failed to start live processing: {}",
                        e
                    )));
                }
            }

            // Spawn MQTT subscriber for each live window
            for window in &live_windows {
                let config = MqttSubscriberConfig {
                    host: "localhost".to_string(),
                    port: 1883,
                    client_id: format!("janus_live_{}_{}", query_id.clone(), window.stream_name),
                    keep_alive_secs: 30,
                    topic: "sensors".to_string(), // TODO: map from stream name or config
                    stream_uri: window.stream_name.clone(),
                    window_graph: window.window_name.clone(),
                };

                let subscriber = Arc::new(MqttSubscriber::new(config));
                let subscriber_clone = Arc::clone(&subscriber);
                let processor_clone = Arc::clone(&live_processor);

                // Spawn MQTT subscriber in a separate thread
                let sub_handle = thread::spawn(move || {
                    if let Err(e) = subscriber_clone.start(processor_clone) {
                        eprintln!("MQTT subscriber error: {}", e);
                    }
                });

                mqtt_subscribers.push(subscriber);
                mqtt_subscriber_handle = Some(sub_handle);
            }

            // Spawn live worker thread to receive results
            let processor_for_worker = Arc::clone(&live_processor);
            let handle = thread::spawn(move || {
                let converter = ResultConverter::new(query_id_clone);
                println!("Live worker thread started");
                let mut results_sent = 0;

                // Continuously receive live results
                loop {
                    // Check for shutdown signal
                    if shutdown_rx.try_recv().is_ok() {
                        println!("Live worker received shutdown signal");
                        break;
                    }

                    let processor = processor_for_worker.lock().unwrap();
                    match processor.try_receive_result() {
                        Ok(Some(binding)) => {
                            println!("Live worker received binding: {:?}", binding);
                            let result = converter.from_live_binding(binding);
                            if tx.send(result).is_err() {
                                println!("Live worker: channel closed, exiting");
                                break;
                            }
                            results_sent += 1;
                            println!("Live worker sent result #{}", results_sent);
                        }
                        Ok(None) => {
                            // No result available, release lock and sleep briefly
                            drop(processor);
                            thread::sleep(std::time::Duration::from_millis(10));
                        }
                        Err(e) => {
                            eprintln!("Live processing error: {}", e);
                            break;
                        }
                    }
                }
                println!("Live worker thread exiting. Sent {} results", results_sent);
            });

            shutdown_senders.push(shutdown_tx);
            Some(handle)
        } else {
            None
        };

        // 6. Store running query information
        let running = RunningQuery {
            metadata,
            status: Arc::new(RwLock::new(ExecutionStatus::Running)),
            primary_sender: result_tx,
            subscribers: vec![],
            historical_handles,
            live_handle,
            mqtt_subscriber_handle,
            shutdown_senders,
            mqtt_subscribers,
        };

        {
            let mut running_map = self.running.lock().unwrap();
            running_map.insert(query_id.clone(), running);
        }

        // 7. Return handle for receiving results
        Ok(QueryHandle { query_id: query_id.clone(), receiver: result_rx })
    }

    /// Stop a running query.
    ///
    /// Sends shutdown signals to all worker threads and waits for them to complete.
    ///
    /// # Arguments
    ///
    /// * `query_id` - The ID of the query to stop
    pub fn stop_query(&self, query_id: &QueryId) -> Result<(), JanusApiError> {
        let mut running_map = self.running.lock().unwrap();

        let running = running_map.remove(query_id).ok_or_else(|| {
            JanusApiError::ExecutionError(format!("Query '{}' is not running", query_id))
        })?;

        // Send shutdown signals
        for shutdown_tx in running.shutdown_senders {
            let _ = shutdown_tx.send(());
        }

        // Stop MQTT subscribers
        for subscriber in &running.mqtt_subscribers {
            subscriber.stop();
        }

        // Update status
        if let Ok(mut status) = running.status.write() {
            *status = ExecutionStatus::Stopped;
        }

        Ok(())
    }

    /// Check if a query is currently running.
    ///
    /// # Arguments
    ///
    /// * `query_id` - The ID of the query to check
    pub fn is_running(&self, query_id: &QueryId) -> bool {
        let running_map = self.running.lock().unwrap();
        running_map.contains_key(query_id)
    }

    /// Get the status of a running query.
    ///
    /// # Arguments
    ///
    /// * `query_id` - The ID of the query
    pub fn get_query_status(&self, query_id: &QueryId) -> Option<ExecutionStatus> {
        let running_map = self.running.lock().unwrap();
        running_map
            .get(query_id)
            .and_then(|running| running.status.read().ok().map(|s| s.clone()))
    }
}
