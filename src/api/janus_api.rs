use crate::{
    core::RDFEvent,
    execution::{HistoricalExecutor, ResultConverter},
    parsing::janusql_parser::{
        BaselineDefinition, BaselineGraphTemplate, GraphTermTemplate, JanusQLParser,
        ParsedJanusQuery, TripleTemplate, WindowType,
    },
    querying::oxigraph_adapter::OxigraphAdapter,
    registry::{
        baseline_registry::{BaselineRegistry, BaselineSnapshot},
        query_registry::{BaselineBootstrapMode, QueryId, QueryMetadata, QueryRegistry},
    },
    storage::segmented_storage::StreamingSegmentedStorage,
    stream::{
        live_stream_processing::{
            DynamicStaticQuadProvider, LiveStreamProcessing, LiveStreamProcessingError,
        },
        mqtt_subscriber::{MqttSubscriber, MqttSubscriberConfig},
    },
};
use oxigraph::model::{BlankNode, GraphName, Literal, NamedNode, NamedOrBlankNode, Quad, Term};
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc, Mutex, RwLock,
    },
    thread,
};

const JANUS_BASELINE_NS: &str = "https://janus.rs/baseline#";

#[derive(Debug, Clone)]
struct BaselineAggregate {
    last_value: String,
    numeric_sum: f64,
    numeric_count: usize,
    all_numeric: bool,
}

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
    // Query-defined baselines are evaluated at startup and stored here for
    // inspection/debugging as the latest SELECT-result snapshot rows.
    query_defined_baselines: Arc<RwLock<HashMap<String, Vec<HashMap<String, String>>>>>,
    baseline_registry: Arc<BaselineRegistry>,
    // Primary sender used to send the results to the main subscriber
    primary_sender: Sender<QueryResult>,
    // Additional senders for other subscribers (if any)
    subscribers: Vec<Sender<QueryResult>>,
    // thread handles for historical and live workers
    historical_handles: Vec<thread::JoinHandle<()>>,
    baseline_handle: Option<thread::JoinHandle<()>>,
    live_handle: Option<thread::JoinHandle<()>>,
    mqtt_subscriber_handles: Vec<thread::JoinHandle<()>>,
    // shutdown sender signals used to stop the workers
    shutdown_senders: Vec<Sender<()>>,
    // MQTT subscriber instances (for stopping)
    mqtt_subscribers: Vec<Arc<MqttSubscriber>>,
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
        self.register_query_with_baseline_mode(query_id, janusql, BaselineBootstrapMode::Aggregate)
    }

    pub fn register_query_with_baseline_mode(
        &self,
        query_id: QueryId,
        janusql: &str,
        baseline_mode: BaselineBootstrapMode,
    ) -> Result<QueryMetadata, JanusApiError> {
        let parsed = self.parser.parse(janusql).map_err(|e| {
            JanusApiError::ParseError(format!("Failed to parse JanusQL query: {}", e))
        })?;
        let metadata = self
            .registry
            .register(query_id.clone(), janusql.to_string(), parsed, baseline_mode)
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
        let effective_baseline_mode = parsed
            .baseline
            .as_ref()
            .map(|baseline| baseline.mode)
            .unwrap_or(metadata.baseline_mode);
        let effective_baseline_window =
            parsed.baseline.as_ref().map(|baseline| baseline.window_name.clone());
        validate_query_defined_baseline_access(parsed)?;
        validate_query_defined_baseline_step_alignment(parsed)?;
        let requires_async_baseline_warmup = !parsed.live_windows.is_empty()
            && !parsed.historical_windows.is_empty()
            && parsed.baseline.is_some();
        let mut historical_handles = Vec::new();
        let mut shutdown_senders = Vec::new();
        let initial_status = if requires_async_baseline_warmup {
            ExecutionStatus::WarmingBaseline
        } else {
            ExecutionStatus::Running
        };
        let status = Arc::new(RwLock::new(initial_status.clone()));

        // 4. Spawn historical worker threads (one per historical window)
        for (i, window) in parsed.historical_windows.iter().enumerate() {
            // Get corresponding SPARQL query
            let Some(sparql_query) = parsed.sparql_queries.get(i).cloned() else {
                // Query-defined baselines may reference historical windows that do not appear
                // in the registered query WHERE clause, so there is no standalone historical
                // worker query to run for them.
                continue;
            };

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
        let mut mqtt_subscriber_handles = Vec::new();
        let mut baseline_handle = None;
        let query_defined_baselines = Arc::new(RwLock::new(HashMap::new()));
        let baseline_registry = Arc::new(BaselineRegistry::new());

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
                if !parsed.ast.baseline_uses.is_empty() {
                    initialize_fixed_query_defined_baselines(
                        &self.storage,
                        parsed,
                        &baseline_registry,
                        &query_defined_baselines,
                    )?;
                    let provider = build_query_defined_baseline_provider(
                        Arc::clone(&self.storage),
                        parsed.clone(),
                        Arc::clone(&baseline_registry),
                        Arc::clone(&query_defined_baselines),
                    );
                    processor.set_dynamic_static_quad_provider(provider).map_err(|e| {
                        JanusApiError::LiveProcessingError(format!(
                            "Failed to register dynamic baseline provider: {}",
                            e
                        ))
                    })?;
                }
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

            if requires_async_baseline_warmup {
                let storage = Arc::clone(&self.storage);
                let parsed_clone = parsed.clone();
                let processor_for_baseline = Arc::clone(&live_processor);
                let status_for_baseline = Arc::clone(&status);
                let registry_for_baseline = Arc::clone(&self.registry);
                let query_id_for_baseline = query_id.clone();
                let baseline_mode = effective_baseline_mode;
                let baseline_window = effective_baseline_window.clone();
                let (baseline_shutdown_tx, baseline_shutdown_rx) = mpsc::channel::<()>();

                baseline_handle =
                    Some(thread::spawn(move || {
                        match collect_query_baseline_statements(
                            &storage,
                            &parsed_clone,
                            baseline_mode,
                            baseline_window.as_deref(),
                            &baseline_shutdown_rx,
                        ) {
                            Ok(statements) => {
                                if baseline_shutdown_rx.try_recv().is_ok() {
                                    return;
                                }

                                if let Ok(mut processor) = processor_for_baseline.lock() {
                                    if let Err(err) = materialize_static_baseline_statements(
                                        &mut processor,
                                        &statements,
                                    ) {
                                        eprintln!("Async baseline materialization error: {}", err);
                                        if let Ok(mut state) = status_for_baseline.write() {
                                            *state = ExecutionStatus::Failed(err.to_string());
                                        }
                                        return;
                                    }
                                }

                                if let Ok(mut state) = status_for_baseline.write() {
                                    if *state == ExecutionStatus::WarmingBaseline {
                                        *state = ExecutionStatus::Running;
                                    }
                                }
                                let _ = registry_for_baseline
                                    .set_status(&query_id_for_baseline, "Running");
                            }
                            Err(err) => {
                                eprintln!("Async baseline warm-up error: {}", err);
                                if let Ok(mut state) = status_for_baseline.write() {
                                    *state = ExecutionStatus::Failed(err.to_string());
                                }
                                let _ = registry_for_baseline
                                    .set_status(&query_id_for_baseline, format!("Failed({err})"));
                            }
                        }
                    }));

                shutdown_senders.push(baseline_shutdown_tx);
            } else if let Ok(mut state) = status.write() {
                *state = ExecutionStatus::Running;
            }

            // Spawn MQTT subscriber for each live window
            for window in &live_windows {
                let (host, port, topic) = parse_mqtt_uri(&window.stream_name);

                let config = MqttSubscriberConfig {
                    host,
                    port,
                    client_id: format!("janus_live_{}_{}", query_id.clone(), window.stream_name),
                    keep_alive_secs: 30,
                    topic,
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
                mqtt_subscriber_handles.push(sub_handle);
            }

            // Spawn live worker thread to receive results
            let processor_for_worker = Arc::clone(&live_processor);
            let handle = thread::spawn(move || {
                let converter = ResultConverter::new(query_id_clone);

                loop {
                    if shutdown_rx.try_recv().is_ok() {
                        break;
                    }

                    let processor = processor_for_worker.lock().unwrap();
                    match processor.try_receive_result() {
                        Ok(Some(binding)) => {
                            let result = converter.from_live_binding(binding);
                            if tx.send(result).is_err() {
                                break;
                            }
                        }
                        Ok(None) => {
                            drop(processor);
                            thread::sleep(std::time::Duration::from_millis(10));
                        }
                        Err(e) => {
                            eprintln!("Live processing error: {}", e);
                            break;
                        }
                    }
                }
            });

            shutdown_senders.push(shutdown_tx);
            Some(handle)
        } else {
            None
        };

        self.registry.increment_execution_count(query_id).map_err(|e| {
            JanusApiError::RegistryError(format!(
                "Failed to increment execution count for '{}': {}",
                query_id, e
            ))
        })?;
        self.registry
            .set_status(query_id, format!("{:?}", initial_status))
            .map_err(|e| {
                JanusApiError::RegistryError(format!(
                    "Failed to update query status for '{}': {}",
                    query_id, e
                ))
            })?;

        // 6. Store running query information
        let running = RunningQuery {
            metadata,
            status,
            query_defined_baselines,
            baseline_registry,
            primary_sender: result_tx,
            subscribers: vec![],
            historical_handles,
            baseline_handle,
            live_handle,
            mqtt_subscriber_handles,
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
        drop(running_map);

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
        self.registry.set_status(query_id, "Stopped").map_err(|e| {
            JanusApiError::RegistryError(format!(
                "Failed to update query status for '{}': {}",
                query_id, e
            ))
        })?;

        for handle in running.historical_handles {
            let _ = handle.join();
        }
        if let Some(handle) = running.baseline_handle {
            let _ = handle.join();
        }
        if let Some(handle) = running.live_handle {
            let _ = handle.join();
        }
        for handle in running.mqtt_subscriber_handles {
            let _ = handle.join();
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

    pub fn get_query_defined_baseline_bindings(
        &self,
        query_id: &QueryId,
    ) -> Option<HashMap<String, Vec<HashMap<String, String>>>> {
        let running_map = self.running.lock().unwrap();
        let running = running_map.get(query_id)?;
        running.query_defined_baselines.read().ok().map(|bindings| bindings.clone())
    }
}

fn collect_query_baseline_statements(
    storage: &Arc<StreamingSegmentedStorage>,
    parsed: &crate::parsing::janusql_parser::ParsedJanusQuery,
    baseline_mode: BaselineBootstrapMode,
    baseline_window_name: Option<&str>,
    shutdown_rx: &Receiver<()>,
) -> Result<Vec<(String, String, String)>, JanusApiError> {
    if parsed.live_windows.is_empty() || parsed.historical_windows.is_empty() {
        return Ok(Vec::new());
    }

    let executor = HistoricalExecutor::new(Arc::clone(storage), OxigraphAdapter::new());
    let mut statements = Vec::new();

    for (index, window) in parsed.historical_windows.iter().enumerate() {
        if shutdown_rx.try_recv().is_ok() {
            return Ok(Vec::new());
        }
        if baseline_window_name.is_some_and(|name| name != window.window_name) {
            continue;
        }

        let Some(sparql_query) = parsed.sparql_queries.get(index) else {
            // Query-defined baselines may be the only consumer of a historical window.
            // In that case there is no main historical SPARQL query to materialize here.
            continue;
        };

        match window.window_type {
            WindowType::HistoricalFixed => {
                let bindings = executor.execute_fixed_window(window, sparql_query)?;
                statements.extend(baseline_statements_from_bindings(&bindings));
            }
            WindowType::HistoricalSliding => {
                statements.extend(collect_sliding_window_baseline_statements(
                    &executor,
                    window,
                    sparql_query,
                    baseline_mode,
                    shutdown_rx,
                )?);
            }
            WindowType::Live => {}
        }
    }

    Ok(statements)
}

fn initialize_fixed_query_defined_baselines(
    storage: &Arc<StreamingSegmentedStorage>,
    parsed: &ParsedJanusQuery,
    baseline_registry: &Arc<BaselineRegistry>,
    latest_rows: &Arc<RwLock<HashMap<String, Vec<HashMap<String, String>>>>>,
) -> Result<(), JanusApiError> {
    for definition in &parsed.ast.baseline_definitions {
        let source_window = find_baseline_source_window(parsed, definition)?;
        if source_window.window_type != WindowType::HistoricalFixed {
            continue;
        }

        let evaluation_time = source_window.end.unwrap_or_default();
        let snapshot = load_or_compute_baseline_snapshot(
            storage,
            parsed,
            definition,
            evaluation_time,
            baseline_registry,
        )?;
        store_latest_baseline_rows(latest_rows, &snapshot);
    }

    Ok(())
}

fn build_query_defined_baseline_provider(
    storage: Arc<StreamingSegmentedStorage>,
    parsed: ParsedJanusQuery,
    baseline_registry: Arc<BaselineRegistry>,
    latest_rows: Arc<RwLock<HashMap<String, Vec<HashMap<String, String>>>>>,
) -> DynamicStaticQuadProvider {
    Arc::new(move |evaluation_time| {
        resolve_query_defined_baseline_quads_at(
            &storage,
            &parsed,
            &baseline_registry,
            &latest_rows,
            evaluation_time,
        )
        .map_err(|err| LiveStreamProcessingError::from(err.to_string()))
    })
}

fn resolve_query_defined_baseline_quads_at(
    storage: &Arc<StreamingSegmentedStorage>,
    parsed: &ParsedJanusQuery,
    baseline_registry: &Arc<BaselineRegistry>,
    latest_rows: &Arc<RwLock<HashMap<String, Vec<HashMap<String, String>>>>>,
    evaluation_time: u64,
) -> Result<Vec<Quad>, JanusApiError> {
    let mut materialized = Vec::new();
    let mut seen = HashSet::new();

    for baseline_use in &parsed.ast.baseline_uses {
        if !seen.insert(baseline_use.name.clone()) {
            continue;
        }

        let definition = parsed
            .ast
            .baseline_definitions
            .iter()
            .find(|definition| definition.name == baseline_use.name)
            .ok_or_else(|| {
                JanusApiError::ExecutionError(format!(
                    "USING BASELINE references missing baseline definition '{}'",
                    baseline_use.name
                ))
            })?;
        let template = parsed
            .baseline_graph_templates
            .iter()
            .find(|template| template.baseline_name == baseline_use.name)
            .ok_or_else(|| {
                JanusApiError::ExecutionError(format!(
                    "USING BASELINE '{}' requires a matching GRAPH reference in the live query",
                    baseline_use.name
                ))
            })?;
        let snapshot = load_or_compute_baseline_snapshot(
            storage,
            parsed,
            definition,
            evaluation_time,
            baseline_registry,
        )?;
        store_latest_baseline_rows(latest_rows, &snapshot);
        materialized
            .extend(materialize_baseline_snapshot_as_quads(definition, template, &snapshot)?);
    }

    Ok(materialized)
}

fn load_or_compute_baseline_snapshot(
    storage: &Arc<StreamingSegmentedStorage>,
    parsed: &ParsedJanusQuery,
    definition: &BaselineDefinition,
    evaluation_time: u64,
    baseline_registry: &Arc<BaselineRegistry>,
) -> Result<BaselineSnapshot, JanusApiError> {
    let source_window = find_baseline_source_window(parsed, definition)?;
    let generated_query = parsed
        .generated_baseline_queries
        .iter()
        .find(|generated| generated.name == definition.name)
        .ok_or_else(|| {
            JanusApiError::ExecutionError(format!(
                "Missing generated baseline query for '{}'",
                definition.name
            ))
        })?;

    let resolved_valid_at = match source_window.window_type {
        WindowType::HistoricalFixed => source_window.end.unwrap_or(evaluation_time),
        WindowType::HistoricalSliding => evaluation_time,
        WindowType::Live => {
            return Err(JanusApiError::ExecutionError(format!(
                "Baseline '{}' cannot use live window '{}'",
                definition.name, source_window.window_name
            )))
        }
    };

    if let Some(snapshot) = baseline_registry.get_snapshot(&definition.name, resolved_valid_at) {
        return Ok(snapshot);
    }
    if source_window.window_type == WindowType::HistoricalFixed {
        if let Some(snapshot) = baseline_registry.get_latest_snapshot(&definition.name) {
            return Ok(snapshot);
        }
    }

    let (window_start, window_end) =
        source_window.resolve_historical_bounds(evaluation_time).ok_or_else(|| {
            JanusApiError::ExecutionError(format!(
                "Failed to resolve historical bounds for baseline '{}' using window '{}'",
                definition.name, source_window.window_name
            ))
        })?;

    let executor = HistoricalExecutor::new(Arc::clone(storage), OxigraphAdapter::new());
    let rows =
        executor.execute_window_bounds(window_start, window_end, &generated_query.sparql_query)?;
    let snapshot = BaselineSnapshot {
        baseline_id: definition.name.clone(),
        valid_at: resolved_valid_at,
        source_window: definition.source_window.clone(),
        window_start,
        window_end,
        variables: generated_query.output_variables.clone(),
        rows,
    };
    baseline_registry.insert_snapshot(snapshot.clone());
    Ok(snapshot)
}

fn find_baseline_source_window<'a>(
    parsed: &'a ParsedJanusQuery,
    definition: &BaselineDefinition,
) -> Result<&'a crate::parsing::janusql_parser::WindowDefinition, JanusApiError> {
    parsed
        .historical_windows
        .iter()
        .find(|window| window.window_name == definition.source_window)
        .ok_or_else(|| {
            JanusApiError::ExecutionError(format!(
                "Missing historical source window '{}' for baseline '{}'",
                definition.source_window, definition.name
            ))
        })
}

fn store_latest_baseline_rows(
    latest_rows: &Arc<RwLock<HashMap<String, Vec<HashMap<String, String>>>>>,
    snapshot: &BaselineSnapshot,
) {
    if let Ok(mut stored) = latest_rows.write() {
        stored.insert(snapshot.baseline_id.clone(), snapshot.rows.clone());
    }
}

fn materialize_baseline_snapshot_as_quads(
    baseline_definition: &BaselineDefinition,
    baseline_graph_template: &BaselineGraphTemplate,
    snapshot: &BaselineSnapshot,
) -> Result<Vec<Quad>, JanusApiError> {
    materialize_baseline_bindings_as_quads(
        &snapshot.baseline_id,
        baseline_definition,
        baseline_graph_template,
        &snapshot.rows,
    )
}

#[allow(dead_code)]
fn collect_query_defined_baseline_bindings(
    storage: &Arc<StreamingSegmentedStorage>,
    parsed: &crate::parsing::janusql_parser::ParsedJanusQuery,
    shutdown_rx: &Receiver<()>,
) -> Result<HashMap<String, Vec<HashMap<String, String>>>, JanusApiError> {
    let executor = HistoricalExecutor::new(Arc::clone(storage), OxigraphAdapter::new());
    let mut baseline_results = HashMap::new();

    for generated in &parsed.generated_baseline_queries {
        if shutdown_rx.try_recv().is_ok() {
            return Ok(HashMap::new());
        }

        let source_window = parsed
            .historical_windows
            .iter()
            .find(|window| window.window_name == generated.source_window)
            .ok_or_else(|| {
                JanusApiError::ExecutionError(format!(
                    "Missing historical source window '{}' for generated baseline '{}'",
                    generated.source_window, generated.name
                ))
            })?;

        let bindings = execute_generated_baseline_query(
            &executor,
            source_window,
            &generated.sparql_query,
            shutdown_rx,
        )?;
        baseline_results.insert(generated.name.clone(), bindings);
    }

    Ok(baseline_results)
}

#[allow(dead_code)]
fn evaluate_and_materialize_query_defined_baselines(
    storage: &Arc<StreamingSegmentedStorage>,
    parsed: &crate::parsing::janusql_parser::ParsedJanusQuery,
    shutdown_rx: &Receiver<()>,
) -> Result<(HashMap<String, Vec<HashMap<String, String>>>, Vec<Quad>), JanusApiError> {
    let bindings_by_name = collect_query_defined_baseline_bindings(storage, parsed, shutdown_rx)?;
    let quads = materialize_query_defined_baseline_quads(parsed, &bindings_by_name)?;
    Ok((bindings_by_name, quads))
}

#[allow(dead_code)]
fn execute_generated_baseline_query(
    executor: &HistoricalExecutor,
    window: &crate::parsing::janusql_parser::WindowDefinition,
    sparql_query: &str,
    shutdown_rx: &Receiver<()>,
) -> Result<Vec<HashMap<String, String>>, JanusApiError> {
    match window.window_type {
        WindowType::HistoricalFixed => executor.execute_fixed_window(window, sparql_query),
        WindowType::HistoricalSliding => {
            let mut latest_bindings = Vec::new();

            for window_result in executor.execute_sliding_windows(window, sparql_query) {
                if shutdown_rx.try_recv().is_ok() {
                    return Ok(Vec::new());
                }
                latest_bindings = window_result?;
            }

            Ok(latest_bindings)
        }
        WindowType::Live => Err(JanusApiError::ExecutionError(format!(
            "Generated baseline query cannot execute on live window '{}'",
            window.window_name
        ))),
    }
}

fn collect_sliding_window_baseline_statements(
    executor: &HistoricalExecutor,
    window: &crate::parsing::janusql_parser::WindowDefinition,
    sparql_query: &str,
    mode: BaselineBootstrapMode,
    shutdown_rx: &Receiver<()>,
) -> Result<Vec<(String, String, String)>, JanusApiError> {
    let mut accumulator = HashMap::new();
    let mut saw_window = false;

    for window_result in executor.execute_sliding_windows(window, sparql_query) {
        if shutdown_rx.try_recv().is_ok() {
            return Ok(Vec::new());
        }
        let bindings = window_result?;
        saw_window = true;

        if mode == BaselineBootstrapMode::Last {
            accumulator.clear();
        }

        accumulate_bindings_into_baseline(&mut accumulator, &bindings);
    }

    if !saw_window {
        return Ok(Vec::new());
    }

    Ok(baseline_statements_from_accumulator(&accumulator))
}

#[allow(dead_code)]
fn materialize_query_defined_baseline_quads(
    parsed: &crate::parsing::janusql_parser::ParsedJanusQuery,
    bindings_by_name: &HashMap<String, Vec<HashMap<String, String>>>,
) -> Result<Vec<Quad>, JanusApiError> {
    let mut materialized = Vec::new();
    let mut seen = HashSet::new();

    for baseline_use in &parsed.ast.baseline_uses {
        if !seen.insert(baseline_use.name.clone()) {
            continue;
        }

        // The GRAPH template is the materialization contract. We use it instead of
        // SELECT alias heuristics because the template explicitly states the RDF
        // shape that should be injected into the live static store.
        let definition = parsed
            .ast
            .baseline_definitions
            .iter()
            .find(|definition| definition.name == baseline_use.name)
            .ok_or_else(|| {
                JanusApiError::ExecutionError(format!(
                    "USING BASELINE references missing baseline definition '{}'",
                    baseline_use.name
                ))
            })?;
        let bindings = bindings_by_name.get(&baseline_use.name).ok_or_else(|| {
            JanusApiError::ExecutionError(format!(
                "USING BASELINE references missing evaluated baseline '{}'",
                baseline_use.name
            ))
        })?;
        let template = parsed
            .baseline_graph_templates
            .iter()
            .find(|template| template.baseline_name == baseline_use.name)
            .ok_or_else(|| {
                JanusApiError::ExecutionError(format!(
                    "USING BASELINE '{}' requires a matching GRAPH reference in the live query",
                    baseline_use.name
                ))
            })?;
        materialized.extend(materialize_baseline_bindings_as_quads(
            &baseline_use.name,
            definition,
            template,
            bindings,
        )?);
    }

    Ok(materialized)
}

#[cfg(test)]
fn materialize_bindings_as_static_baseline(
    processor: &mut LiveStreamProcessing,
    bindings: &[HashMap<String, String>],
) -> Result<(), JanusApiError> {
    let statements = baseline_statements_from_bindings(bindings);
    materialize_static_baseline_statements(processor, &statements)
}

#[allow(dead_code)]
fn materialize_static_quads(
    processor: &mut LiveStreamProcessing,
    quads: &[Quad],
) -> Result<(), JanusApiError> {
    for quad in quads {
        processor.add_static_quad(quad.clone());
    }
    Ok(())
}

fn materialize_static_baseline_statements(
    processor: &mut LiveStreamProcessing,
    statements: &[(String, String, String)],
) -> Result<(), JanusApiError> {
    for (subject, predicate, object) in statements {
        processor
            .add_static_data(RDFEvent::new(0, subject, predicate, object, ""))
            .map_err(|e| {
                JanusApiError::LiveProcessingError(format!(
                    "Failed to materialize baseline statement '{} {} {}': {}",
                    subject, predicate, object, e
                ))
            })?;
    }
    Ok(())
}

fn materialize_baseline_bindings_as_quads(
    baseline_name: &str,
    baseline_definition: &crate::parsing::janusql_parser::BaselineDefinition,
    baseline_graph_template: &BaselineGraphTemplate,
    bindings: &[HashMap<String, String>],
) -> Result<Vec<Quad>, JanusApiError> {
    let graph_name = GraphName::NamedNode(NamedNode::new(baseline_name).map_err(|e| {
        JanusApiError::ExecutionError(format!(
            "Invalid baseline graph name '{}': {}",
            baseline_name, e
        ))
    })?);

    let mut quads = Vec::new();
    for binding in bindings {
        for triple in &baseline_graph_template.triples {
            let subject = resolve_subject_template_term(baseline_name, triple, binding)?;
            let predicate = resolve_predicate_template_term(baseline_name, triple, binding)?;
            let object = resolve_object_template_term(baseline_name, triple, binding)?;
            quads.push(Quad::new(subject, predicate, object, graph_name.clone()));
        }
    }

    Ok(quads)
}

fn baseline_statements_from_bindings(
    bindings: &[HashMap<String, String>],
) -> Vec<(String, String, String)> {
    let mut accumulator = HashMap::new();
    accumulate_bindings_into_baseline(&mut accumulator, bindings);
    baseline_statements_from_accumulator(&accumulator)
}

fn accumulate_bindings_into_baseline(
    accumulator: &mut HashMap<(String, String), BaselineAggregate>,
    bindings: &[HashMap<String, String>],
) {
    for binding in bindings {
        let Some((anchor_var, anchor_subject)) = select_binding_anchor(binding) else {
            continue;
        };

        let mut variables = binding.keys().cloned().collect::<Vec<_>>();
        variables.sort_unstable();

        for var in variables {
            if var == anchor_var {
                continue;
            }

            let Some(raw_value) = binding.get(&var) else {
                continue;
            };

            let normalized = normalize_binding_term(raw_value);
            let key = (anchor_subject.clone(), var);
            let entry = accumulator.entry(key).or_insert_with(|| BaselineAggregate {
                last_value: normalized.clone(),
                numeric_sum: 0.0,
                numeric_count: 0,
                all_numeric: true,
            });

            entry.last_value.clone_from(&normalized);
            if let Ok(value) = normalized.parse::<f64>() {
                entry.numeric_sum += value;
                entry.numeric_count += 1;
            } else {
                entry.all_numeric = false;
            }
        }
    }
}

fn baseline_statements_from_accumulator(
    accumulator: &HashMap<(String, String), BaselineAggregate>,
) -> Vec<(String, String, String)> {
    let mut entries = accumulator.iter().collect::<Vec<_>>();
    entries.sort_by(|((left_subject, left_var), _), ((right_subject, right_var), _)| {
        match left_subject.cmp(right_subject) {
            Ordering::Equal => left_var.cmp(right_var),
            other => other,
        }
    });

    entries
        .into_iter()
        .map(|((subject, var), aggregate)| {
            let predicate = format!("{JANUS_BASELINE_NS}{var}");
            let object = if aggregate.all_numeric && aggregate.numeric_count > 0 {
                (aggregate.numeric_sum / aggregate.numeric_count as f64).to_string()
            } else {
                aggregate.last_value.clone()
            };
            (subject.clone(), predicate, object)
        })
        .collect()
}

fn select_binding_anchor(binding: &HashMap<String, String>) -> Option<(String, String)> {
    for preferred in ["sensor", "subject", "entity", "s"] {
        if let Some(value) = binding.get(preferred).and_then(|raw| normalize_iri_term(raw)) {
            return Some((preferred.to_string(), value));
        }
    }

    let mut entries = binding.iter().collect::<Vec<_>>();
    entries.sort_by(|(left_name, _), (right_name, _)| {
        if left_name == right_name {
            Ordering::Equal
        } else {
            left_name.cmp(right_name)
        }
    });

    entries
        .into_iter()
        .find_map(|(name, raw)| normalize_iri_term(raw).map(|value| (name.clone(), value)))
}

fn validate_query_defined_baseline_access(
    parsed: &crate::parsing::janusql_parser::ParsedJanusQuery,
) -> Result<(), JanusApiError> {
    for baseline_use in &parsed.ast.baseline_uses {
        let template = parsed
            .baseline_graph_templates
            .iter()
            .find(|template| template.baseline_name == baseline_use.name)
            .ok_or_else(|| {
                JanusApiError::ExecutionError(format!(
                    "USING BASELINE '{}' requires a matching GRAPH reference in the live query",
                    baseline_use.name
                ))
            })?;
        let definition = parsed
            .ast
            .baseline_definitions
            .iter()
            .find(|definition| definition.name == baseline_use.name)
            .ok_or_else(|| {
                JanusApiError::ExecutionError(format!(
                    "USING BASELINE references missing baseline definition '{}'",
                    baseline_use.name
                ))
            })?;
        validate_baseline_graph_template(definition, template)?;
    }

    Ok(())
}

fn validate_query_defined_baseline_step_alignment(
    parsed: &crate::parsing::janusql_parser::ParsedJanusQuery,
) -> Result<(), JanusApiError> {
    if parsed.live_windows.is_empty() {
        return Ok(());
    }

    let live_step = parsed.live_windows[0].slide;
    if parsed.live_windows.iter().any(|window| window.slide != live_step) {
        return Err(JanusApiError::ExecutionError(
            "Queries with multiple live STEP values are not supported with USING BASELINE"
                .to_string(),
        ));
    }

    for definition in &parsed.ast.baseline_definitions {
        let Some(source_window) = parsed
            .historical_windows
            .iter()
            .find(|window| window.window_name == definition.source_window)
        else {
            continue;
        };

        if source_window.window_type == WindowType::HistoricalSliding
            && source_window.slide != live_step
        {
            return Err(JanusApiError::ExecutionError(format!(
                "Sliding historical baseline window '{}' STEP {} must match live STEP {}",
                source_window.window_name, source_window.slide, live_step
            )));
        }
    }

    Ok(())
}

fn validate_baseline_graph_template(
    baseline_definition: &crate::parsing::janusql_parser::BaselineDefinition,
    baseline_graph_template: &BaselineGraphTemplate,
) -> Result<(), JanusApiError> {
    let output_variables = baseline_definition
        .output_variables
        .iter()
        .map(|variable| variable.trim_start_matches('?'))
        .collect::<HashSet<_>>();

    for triple in &baseline_graph_template.triples {
        for term in [&triple.subject, &triple.object] {
            if let GraphTermTemplate::Variable(variable_name) = term {
                if !output_variables.contains(variable_name.as_str()) {
                    return Err(JanusApiError::ExecutionError(format!(
                        "GRAPH template for baseline '{}' references variable '?{}' that is not produced by the baseline SELECT output",
                        baseline_graph_template.baseline_name,
                        variable_name
                    )));
                }
            }
        }

        if let GraphTermTemplate::Variable(variable_name) = &triple.predicate {
            return Err(JanusApiError::ExecutionError(format!(
                "GRAPH template for baseline '{}' uses variable predicate '?{}', but predicates must be concrete IRIs for now",
                baseline_graph_template.baseline_name,
                variable_name
            )));
        }
    }

    Ok(())
}

fn resolve_subject_template_term(
    baseline_name: &str,
    triple: &TripleTemplate,
    binding: &HashMap<String, String>,
) -> Result<NamedOrBlankNode, JanusApiError> {
    match &triple.subject {
        GraphTermTemplate::Variable(variable_name) => {
            let raw_value = binding.get(variable_name).ok_or_else(|| {
                JanusApiError::ExecutionError(format!(
                    "Baseline '{}' binding is missing GRAPH template variable '?{}'",
                    baseline_name, variable_name
                ))
            })?;
            parse_subject_term(raw_value).map_err(|e| {
                JanusApiError::ExecutionError(format!(
                    "Failed to materialize baseline '{}' subject from variable '?{}' with value '{}': {}",
                    baseline_name, variable_name, raw_value, e
                ))
            })
        }
        GraphTermTemplate::Iri(iri) => parse_named_or_blank_node(iri).map_err(|e| {
            JanusApiError::ExecutionError(format!(
                "Failed to materialize baseline '{}' subject IRI '{}': {}",
                baseline_name, iri, e
            ))
        }),
        GraphTermTemplate::Literal(raw_literal) => Err(JanusApiError::ExecutionError(format!(
            "GRAPH template for baseline '{}' has a literal subject '{}', but subjects must be IRIs or blank nodes",
            baseline_name, raw_literal
        ))),
    }
}

fn resolve_predicate_template_term(
    baseline_name: &str,
    triple: &TripleTemplate,
    binding: &HashMap<String, String>,
) -> Result<NamedNode, JanusApiError> {
    match &triple.predicate {
        GraphTermTemplate::Iri(iri) => NamedNode::new(iri.clone()).map_err(|e| {
            JanusApiError::ExecutionError(format!(
                "Failed to materialize baseline '{}' predicate '{}': {}",
                baseline_name, iri, e
            ))
        }),
        GraphTermTemplate::Variable(variable_name) => {
            let _ = binding;
            Err(JanusApiError::ExecutionError(format!(
                "GRAPH template for baseline '{}' uses variable predicate '?{}', but predicates must be concrete IRIs for now",
                baseline_name, variable_name
            )))
        }
        GraphTermTemplate::Literal(raw_literal) => Err(JanusApiError::ExecutionError(format!(
            "GRAPH template for baseline '{}' has a literal predicate '{}', but predicates must be IRIs",
            baseline_name, raw_literal
        ))),
    }
}

fn resolve_object_template_term(
    baseline_name: &str,
    triple: &TripleTemplate,
    binding: &HashMap<String, String>,
) -> Result<Term, JanusApiError> {
    match &triple.object {
        GraphTermTemplate::Variable(variable_name) => {
            let raw_value = binding.get(variable_name).ok_or_else(|| {
                JanusApiError::ExecutionError(format!(
                    "Baseline '{}' binding is missing GRAPH template variable '?{}'",
                    baseline_name, variable_name
                ))
            })?;
            parse_term(raw_value).map_err(|e| {
                JanusApiError::ExecutionError(format!(
                    "Failed to materialize baseline '{}' object from variable '?{}' with value '{}': {}",
                    baseline_name, variable_name, raw_value, e
                ))
            })
        }
        GraphTermTemplate::Iri(iri) => parse_term(iri).map_err(|e| {
            JanusApiError::ExecutionError(format!(
                "Failed to materialize baseline '{}' object IRI '{}': {}",
                baseline_name, iri, e
            ))
        }),
        GraphTermTemplate::Literal(raw_literal) => {
            parse_literal_term(raw_literal).map(Term::Literal).map_err(|e| {
                JanusApiError::ExecutionError(format!(
                    "Failed to materialize baseline '{}' literal object '{}': {}",
                    baseline_name, raw_literal, e
                ))
            })
        }
    }
}

fn parse_subject_term(raw: &str) -> Result<NamedOrBlankNode, String> {
    parse_named_or_blank_node(raw)
}

fn parse_term(raw: &str) -> Result<Term, String> {
    if let Some(blank_node) = normalize_blank_node_term(raw) {
        return BlankNode::new(blank_node).map(Term::BlankNode).map_err(|e| e.to_string());
    }
    if let Some(iri) = normalize_iri_term(raw) {
        return NamedNode::new(iri).map(Term::NamedNode).map_err(|e| e.to_string());
    }

    parse_literal_term(raw).map(Term::Literal)
}

fn parse_named_or_blank_node(raw: &str) -> Result<NamedOrBlankNode, String> {
    if let Some(blank_node) = normalize_blank_node_term(raw) {
        return BlankNode::new(blank_node)
            .map(NamedOrBlankNode::BlankNode)
            .map_err(|e| e.to_string());
    }
    if let Some(iri) = normalize_iri_term(raw) {
        return NamedNode::new(iri).map(NamedOrBlankNode::NamedNode).map_err(|e| e.to_string());
    }
    Err(format!("expected IRI or blank node subject but found {}", raw.trim()))
}

fn parse_literal_term(raw: &str) -> Result<Literal, String> {
    let trimmed = raw.trim();
    if !trimmed.starts_with('"') {
        if trimmed.parse::<i64>().is_ok() {
            return Ok(Literal::new_typed_literal(
                trimmed,
                NamedNode::new("http://www.w3.org/2001/XMLSchema#integer").unwrap(),
            ));
        }
        if trimmed.parse::<f64>().is_ok() {
            return Ok(Literal::new_typed_literal(
                trimmed,
                NamedNode::new("http://www.w3.org/2001/XMLSchema#decimal").unwrap(),
            ));
        }
        return Ok(Literal::new_simple_literal(trimmed));
    }

    let (lexical, suffix) = split_literal_lexical_and_suffix(trimmed)?;
    let lexical = unescape_literal_lexical(lexical);

    if let Some(language) = suffix.strip_prefix('@') {
        return Literal::new_language_tagged_literal(lexical, language).map_err(|e| e.to_string());
    }

    if let Some(datatype_iri) = suffix.strip_prefix("^^") {
        let datatype = if datatype_iri.starts_with('<') && datatype_iri.ends_with('>') {
            &datatype_iri[1..datatype_iri.len() - 1]
        } else {
            datatype_iri
        };
        return Ok(Literal::new_typed_literal(
            lexical,
            NamedNode::new(datatype).map_err(|e| e.to_string())?,
        ));
    }

    Ok(Literal::new_simple_literal(lexical))
}

fn split_literal_lexical_and_suffix(raw: &str) -> Result<(&str, &str), String> {
    let mut escaped = false;

    for (index, ch) in raw.char_indices().skip(1) {
        if escaped {
            escaped = false;
            continue;
        }

        match ch {
            '\\' => escaped = true,
            '"' => return Ok((&raw[1..index], raw[index + 1..].trim())),
            _ => {}
        }
    }

    Err(format!("invalid RDF literal '{}'", raw))
}

fn unescape_literal_lexical(raw: &str) -> String {
    raw.replace("\\\"", "\"")
        .replace("\\\\", "\\")
        .replace("\\n", "\n")
        .replace("\\t", "\t")
}

fn normalize_binding_term(raw: &str) -> String {
    normalize_iri_term(raw)
        .or_else(|| normalize_literal_term(raw))
        .unwrap_or_else(|| raw.trim().to_string())
}

fn normalize_iri_term(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.starts_with('<') && trimmed.ends_with('>') && trimmed.len() > 2 {
        Some(trimmed[1..trimmed.len() - 1].to_string())
    } else if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        Some(trimmed.to_string())
    } else {
        None
    }
}

fn normalize_blank_node_term(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    trimmed.strip_prefix("_:").map(str::to_string)
}

fn normalize_literal_term(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if !trimmed.starts_with('"') {
        return None;
    }

    let mut escaped = false;
    for (index, ch) in trimmed.char_indices().skip(1) {
        if escaped {
            escaped = false;
            continue;
        }

        match ch {
            '\\' => escaped = true,
            '"' => {
                let lexical = &trimmed[1..index];
                return Some(
                    lexical
                        .replace("\\\"", "\"")
                        .replace("\\\\", "\\")
                        .replace("\\n", "\n")
                        .replace("\\t", "\t"),
                );
            }
            _ => {}
        }
    }

    None
}

/// Parses an MQTT stream URI into `(host, port, topic)`.
///
/// Handles `mqtt://host:port/topic` and `mqtts://host:port/topic` directly.
/// For any other URI scheme (e.g. `http://example.org/sensors`) it falls back
/// to `localhost:1883` with the last path segment as the topic, keeping all
/// existing queries backward compatible.
fn parse_mqtt_uri(stream_uri: &str) -> (String, u16, String) {
    if stream_uri.starts_with("mqtt://") || stream_uri.starts_with("mqtts://") {
        let without_scheme =
            stream_uri.trim_start_matches("mqtts://").trim_start_matches("mqtt://");

        let (authority, path) = if let Some(slash) = without_scheme.find('/') {
            (&without_scheme[..slash], &without_scheme[slash + 1..])
        } else {
            (without_scheme, "")
        };

        let (host, port) = if let Some(colon) = authority.rfind(':') {
            let port = authority[colon + 1..].parse::<u16>().unwrap_or(1883);
            (authority[..colon].to_string(), port)
        } else {
            (authority.to_string(), 1883u16)
        };

        let topic = if path.is_empty() {
            "default".to_string()
        } else {
            path.to_string()
        };
        return (host, port, topic);
    }

    // Non-mqtt URI: derive topic from last path segment, use localhost:1883.
    let topic = stream_uri
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(stream_uri)
        .to_string();
    ("localhost".to_string(), 1883u16, topic)
}

#[cfg(test)]
mod tests {
    use super::{
        baseline_statements_from_bindings, collect_query_defined_baseline_bindings,
        materialize_baseline_bindings_as_quads, materialize_bindings_as_static_baseline,
        normalize_binding_term, parse_mqtt_uri, validate_baseline_graph_template,
        JANUS_BASELINE_NS,
    };
    use crate::{
        core::RDFEvent,
        execution::ResultConverter,
        extensions::query_options::build_evaluator,
        parsing::janusql_parser::{
            BaselineDefinition, BaselineGraphTemplate, GraphTermTemplate, JanusQLParser,
            TripleTemplate,
        },
        registry::baseline_registry::{BaselineRegistry, BaselineSnapshot},
        storage::{segmented_storage::StreamingSegmentedStorage, util::StreamingConfig},
        stream::live_stream_processing::LiveStreamProcessing,
    };
    use oxigraph::{
        model::{GraphName, Literal, NamedNode, NamedOrBlankNode, Quad, Term},
        sparql::QueryResults,
        store::Store,
    };
    use std::{
        collections::HashMap,
        sync::{mpsc, Arc, RwLock},
        thread,
        time::Duration,
    };

    #[test]
    fn test_parse_mqtt_uri_with_port() {
        let (host, port, topic) = parse_mqtt_uri("mqtt://mybroker:1884/temperature");
        assert_eq!(host, "mybroker");
        assert_eq!(port, 1884);
        assert_eq!(topic, "temperature");
    }

    #[test]
    fn test_parse_mqtt_uri_default_port() {
        let (host, port, topic) = parse_mqtt_uri("mqtt://mybroker/sensors");
        assert_eq!(host, "mybroker");
        assert_eq!(port, 1883);
        assert_eq!(topic, "sensors");
    }

    #[test]
    fn test_parse_mqtts_uri() {
        let (host, port, topic) = parse_mqtt_uri("mqtts://secure-broker:8883/readings");
        assert_eq!(host, "secure-broker");
        assert_eq!(port, 8883);
        assert_eq!(topic, "readings");
    }

    #[test]
    fn test_parse_http_uri_fallback() {
        let (host, port, topic) = parse_mqtt_uri("http://example.org/sensors");
        assert_eq!(host, "localhost");
        assert_eq!(port, 1883);
        assert_eq!(topic, "sensors");
    }

    #[test]
    fn test_parse_http_uri_fallback_trailing_slash() {
        let (host, port, topic) = parse_mqtt_uri("http://example.org/sensors/");
        assert_eq!(host, "localhost");
        assert_eq!(port, 1883);
        assert_eq!(topic, "sensors");
    }

    #[test]
    fn test_normalize_binding_term_strips_iri_and_literal_wrappers() {
        assert_eq!(
            normalize_binding_term("<http://example.org/sensor1>"),
            "http://example.org/sensor1"
        );
        assert_eq!(normalize_binding_term("\"42.5\""), "42.5");
        assert_eq!(
            normalize_binding_term("\"42.5\"^^<http://www.w3.org/2001/XMLSchema#decimal>"),
            "42.5"
        );
    }

    #[test]
    fn test_materialize_query_defined_baseline_bindings_as_quads() {
        let definition = BaselineDefinition {
            name: "http://example.org/dayBaseline".to_string(),
            source_window: "http://example.org/historyDay".to_string(),
            raw_query: String::new(),
            select_clause: "SELECT ?sensor (AVG(?value) AS ?dayAvgValue)".to_string(),
            where_clause: "WHERE { ?sensor <http://example.org/hasValue> ?value . }".to_string(),
            group_by_clause: Some("GROUP BY ?sensor".to_string()),
            output_variables: vec!["?sensor".to_string(), "?dayAvgValue".to_string()],
        };
        let bindings = vec![HashMap::from([
            ("sensor".to_string(), "<http://example.org/s1>".to_string()),
            (
                "dayAvgValue".to_string(),
                "\"42.0\"^^<http://www.w3.org/2001/XMLSchema#decimal>".to_string(),
            ),
        ])];
        let template = BaselineGraphTemplate {
            baseline_name: "http://example.org/dayBaseline".to_string(),
            triples: vec![TripleTemplate {
                subject: GraphTermTemplate::Variable("sensor".to_string()),
                predicate: GraphTermTemplate::Iri("http://example.org/dayAvgValue".to_string()),
                object: GraphTermTemplate::Variable("dayAvgValue".to_string()),
            }],
        };

        let quads = materialize_baseline_bindings_as_quads(
            "http://example.org/dayBaseline",
            &definition,
            &template,
            &bindings,
        )
        .expect("baseline quads should materialize");

        assert_eq!(
            quads,
            vec![Quad::new(
                NamedOrBlankNode::NamedNode(NamedNode::new("http://example.org/s1").unwrap()),
                NamedNode::new("http://example.org/dayAvgValue").unwrap(),
                Term::Literal(Literal::new_typed_literal(
                    "42.0",
                    NamedNode::new("http://www.w3.org/2001/XMLSchema#decimal").unwrap(),
                )),
                GraphName::NamedNode(NamedNode::new("http://example.org/dayBaseline").unwrap()),
            )]
        );
    }

    #[test]
    fn test_materialize_query_defined_baseline_preserves_string_and_language_literals() {
        let definition = BaselineDefinition {
            name: "http://example.org/dayBaseline".to_string(),
            source_window: "http://example.org/historyDay".to_string(),
            raw_query: String::new(),
            select_clause: "SELECT ?sensor ?label ?note".to_string(),
            where_clause: "WHERE { ?sensor ?p ?o . }".to_string(),
            group_by_clause: None,
            output_variables: vec![
                "?sensor".to_string(),
                "?label".to_string(),
                "?note".to_string(),
            ],
        };
        let bindings = vec![HashMap::from([
            ("sensor".to_string(), "<http://example.org/s1>".to_string()),
            ("label".to_string(), "\"pump\"".to_string()),
            ("note".to_string(), "\"bonjour\"@fr".to_string()),
        ])];
        let template = BaselineGraphTemplate {
            baseline_name: "http://example.org/dayBaseline".to_string(),
            triples: vec![
                TripleTemplate {
                    subject: GraphTermTemplate::Variable("sensor".to_string()),
                    predicate: GraphTermTemplate::Iri("http://example.org/label".to_string()),
                    object: GraphTermTemplate::Variable("label".to_string()),
                },
                TripleTemplate {
                    subject: GraphTermTemplate::Variable("sensor".to_string()),
                    predicate: GraphTermTemplate::Iri("http://example.org/note".to_string()),
                    object: GraphTermTemplate::Variable("note".to_string()),
                },
            ],
        };

        let quads = materialize_baseline_bindings_as_quads(
            "http://example.org/dayBaseline",
            &definition,
            &template,
            &bindings,
        )
        .expect("baseline quads should materialize");

        assert_eq!(quads.len(), 2);
        assert!(quads.iter().any(|quad| quad.predicate.as_str() == "http://example.org/label"
            && quad.object == Term::Literal(Literal::new_simple_literal("pump"))));
        assert!(quads.iter().any(|quad| quad.predicate.as_str() == "http://example.org/note"
            && quad.object
                == Term::Literal(Literal::new_language_tagged_literal("bonjour", "fr").unwrap())));
    }

    #[test]
    fn test_materialize_query_defined_baseline_rejects_non_iri_subject() {
        let definition = BaselineDefinition {
            name: "http://example.org/dayBaseline".to_string(),
            source_window: "http://example.org/historyDay".to_string(),
            raw_query: String::new(),
            select_clause: "SELECT ?sensor (AVG(?value) AS ?dayAvgValue)".to_string(),
            where_clause: "WHERE { ?sensor ?p ?value . }".to_string(),
            group_by_clause: None,
            output_variables: vec!["?sensor".to_string(), "?dayAvgValue".to_string()],
        };
        let template = BaselineGraphTemplate {
            baseline_name: "http://example.org/dayBaseline".to_string(),
            triples: vec![TripleTemplate {
                subject: GraphTermTemplate::Variable("sensor".to_string()),
                predicate: GraphTermTemplate::Iri("http://example.org/dayAvgValue".to_string()),
                object: GraphTermTemplate::Variable("dayAvgValue".to_string()),
            }],
        };
        let bindings = vec![HashMap::from([
            ("sensor".to_string(), "\"not-an-iri\"".to_string()),
            (
                "dayAvgValue".to_string(),
                "\"42.0\"^^<http://www.w3.org/2001/XMLSchema#decimal>".to_string(),
            ),
        ])];

        let err = materialize_baseline_bindings_as_quads(
            "http://example.org/dayBaseline",
            &definition,
            &template,
            &bindings,
        )
        .expect_err("materialization should fail for non-IRI subject");

        assert!(err.to_string().contains("expected IRI or blank node subject"));
    }

    #[test]
    fn test_validate_query_defined_baseline_rejects_missing_template_variable() {
        let definition = BaselineDefinition {
            name: "http://example.org/dayBaseline".to_string(),
            source_window: "http://example.org/historyDay".to_string(),
            raw_query: String::new(),
            select_clause: "SELECT ?sensor (AVG(?value) AS ?dayAvgValue)".to_string(),
            where_clause: "WHERE { ?sensor ?p ?value . }".to_string(),
            group_by_clause: Some("GROUP BY ?sensor".to_string()),
            output_variables: vec!["?sensor".to_string(), "?dayAvgValue".to_string()],
        };
        let template = BaselineGraphTemplate {
            baseline_name: "http://example.org/dayBaseline".to_string(),
            triples: vec![TripleTemplate {
                subject: GraphTermTemplate::Variable("sensor".to_string()),
                predicate: GraphTermTemplate::Iri("http://example.org/dayCount".to_string()),
                object: GraphTermTemplate::Variable("dayCount".to_string()),
            }],
        };

        let err = validate_baseline_graph_template(&definition, &template)
            .expect_err("validation should fail when template variable is absent");
        assert!(err.to_string().contains("references variable '?dayCount' that is not produced"));
    }

    #[test]
    fn test_validate_query_defined_baseline_rejects_variable_predicate() {
        let definition = BaselineDefinition {
            name: "http://example.org/dayBaseline".to_string(),
            source_window: "http://example.org/historyDay".to_string(),
            raw_query: String::new(),
            select_clause: "SELECT ?sensor ?pred ?dayAvgValue".to_string(),
            where_clause: "WHERE { ?sensor ?pred ?dayAvgValue . }".to_string(),
            group_by_clause: None,
            output_variables: vec![
                "?sensor".to_string(),
                "?pred".to_string(),
                "?dayAvgValue".to_string(),
            ],
        };
        let template = BaselineGraphTemplate {
            baseline_name: "http://example.org/dayBaseline".to_string(),
            triples: vec![TripleTemplate {
                subject: GraphTermTemplate::Variable("sensor".to_string()),
                predicate: GraphTermTemplate::Variable("pred".to_string()),
                object: GraphTermTemplate::Variable("dayAvgValue".to_string()),
            }],
        };

        let err = validate_baseline_graph_template(&definition, &template)
            .expect_err("variable predicate should be rejected");
        assert!(err.to_string().contains("predicates must be concrete IRIs"));
    }

    #[test]
    fn test_materialize_query_defined_baseline_uses_multiple_template_triples() {
        let definition = BaselineDefinition {
            name: "http://example.org/dayBaseline".to_string(),
            source_window: "http://example.org/historyDay".to_string(),
            raw_query: String::new(),
            select_clause: "SELECT ?sensor ?dayAvgValue ?dayCount".to_string(),
            where_clause: "WHERE { ?sensor ?p ?o . }".to_string(),
            group_by_clause: None,
            output_variables: vec![
                "?sensor".to_string(),
                "?dayAvgValue".to_string(),
                "?dayCount".to_string(),
            ],
        };
        let template = BaselineGraphTemplate {
            baseline_name: "http://example.org/dayBaseline".to_string(),
            triples: vec![
                TripleTemplate {
                    subject: GraphTermTemplate::Variable("sensor".to_string()),
                    predicate: GraphTermTemplate::Iri("http://example.org/dayAvgValue".to_string()),
                    object: GraphTermTemplate::Variable("dayAvgValue".to_string()),
                },
                TripleTemplate {
                    subject: GraphTermTemplate::Variable("sensor".to_string()),
                    predicate: GraphTermTemplate::Iri("http://example.org/dayCount".to_string()),
                    object: GraphTermTemplate::Variable("dayCount".to_string()),
                },
            ],
        };
        let bindings = vec![HashMap::from([
            ("sensor".to_string(), "<http://example.org/s1>".to_string()),
            (
                "dayAvgValue".to_string(),
                "\"42.0\"^^<http://www.w3.org/2001/XMLSchema#decimal>".to_string(),
            ),
            (
                "dayCount".to_string(),
                "\"10\"^^<http://www.w3.org/2001/XMLSchema#integer>".to_string(),
            ),
        ])];

        let quads = materialize_baseline_bindings_as_quads(
            "http://example.org/dayBaseline",
            &definition,
            &template,
            &bindings,
        )
        .expect("template materialization should succeed");

        assert_eq!(quads.len(), 2);
        assert!(quads
            .iter()
            .any(|quad| quad.predicate.as_str() == "http://example.org/dayAvgValue"));
        assert!(quads
            .iter()
            .any(|quad| quad.predicate.as_str() == "http://example.org/dayCount"));
    }

    #[test]
    fn test_materialize_query_defined_baseline_uses_template_predicate_not_variable_name() {
        let definition = BaselineDefinition {
            name: "http://example.org/dayBaseline".to_string(),
            source_window: "http://example.org/historyDay".to_string(),
            raw_query: String::new(),
            select_clause: "SELECT ?sensor ?dayAvgValue".to_string(),
            where_clause: "WHERE { ?sensor ?p ?o . }".to_string(),
            group_by_clause: None,
            output_variables: vec!["?sensor".to_string(), "?dayAvgValue".to_string()],
        };
        let template = BaselineGraphTemplate {
            baseline_name: "http://example.org/dayBaseline".to_string(),
            triples: vec![TripleTemplate {
                subject: GraphTermTemplate::Variable("sensor".to_string()),
                predicate: GraphTermTemplate::Iri(
                    "http://example.org/customBaselineValue".to_string(),
                ),
                object: GraphTermTemplate::Variable("dayAvgValue".to_string()),
            }],
        };
        let bindings = vec![HashMap::from([
            ("sensor".to_string(), "<http://example.org/s1>".to_string()),
            (
                "dayAvgValue".to_string(),
                "\"42.0\"^^<http://www.w3.org/2001/XMLSchema#decimal>".to_string(),
            ),
        ])];

        let quads = materialize_baseline_bindings_as_quads(
            "http://example.org/dayBaseline",
            &definition,
            &template,
            &bindings,
        )
        .expect("template materialization should succeed");

        assert_eq!(quads.len(), 1);
        assert_eq!(quads[0].predicate.as_str(), "http://example.org/customBaselineValue");
    }

    #[test]
    fn test_materialized_baseline_static_data_can_drive_live_extension_functions() {
        let query = format!(
            r#"
                PREFIX ex: <http://example.org/>
                PREFIX janus: <https://janus.rs/fn#>
                PREFIX baseline: <{}>
                REGISTER RStream <output> AS
                SELECT ?sensor ?reading
                FROM NAMED WINDOW ex:w1 ON STREAM ex:stream1 [RANGE 1000 STEP 500]
                WHERE {{
                    WINDOW ex:w1 {{
                        ?sensor ex:hasReading ?reading .
                    }}
                    ?sensor baseline:mean ?mean .
                    ?sensor baseline:sigma ?sigma .
                    FILTER(janus:is_outlier(?reading, ?mean, ?sigma, 3))
                }}
            "#,
            JANUS_BASELINE_NS
        );

        let mut processor = LiveStreamProcessing::new(query).unwrap();
        processor.register_stream("http://example.org/stream1").unwrap();

        let mut binding = HashMap::new();
        binding.insert("sensor".to_string(), "<http://example.org/sensor1>".to_string());
        binding.insert(
            "mean".to_string(),
            "\"25\"^^<http://www.w3.org/2001/XMLSchema#decimal>".to_string(),
        );
        binding.insert(
            "sigma".to_string(),
            "\"2\"^^<http://www.w3.org/2001/XMLSchema#decimal>".to_string(),
        );

        materialize_bindings_as_static_baseline(&mut processor, &[binding]).unwrap();
        processor.start_processing().unwrap();
        processor
            .add_event(
                "http://example.org/stream1",
                RDFEvent::new(
                    0,
                    "http://example.org/sensor1",
                    "http://example.org/hasReading",
                    "40",
                    "",
                ),
            )
            .unwrap();
        processor.close_stream("http://example.org/stream1", 3000).unwrap();
        thread::sleep(Duration::from_millis(300));

        let results = processor.collect_results(None).unwrap();
        assert!(
            results.iter().any(|result| result.bindings.contains("sensor1")),
            "expected live result to join with materialized baseline static data, got {:?}",
            results
        );
    }

    #[test]
    fn test_baseline_statements_from_bindings_aggregate_numeric_values() {
        let bindings = vec![
            HashMap::from([
                ("sensor".to_string(), "<http://example.org/s1>".to_string()),
                (
                    "mean".to_string(),
                    "\"10\"^^<http://www.w3.org/2001/XMLSchema#decimal>".to_string(),
                ),
            ]),
            HashMap::from([
                ("sensor".to_string(), "<http://example.org/s1>".to_string()),
                (
                    "mean".to_string(),
                    "\"20\"^^<http://www.w3.org/2001/XMLSchema#decimal>".to_string(),
                ),
            ]),
        ];

        let statements = baseline_statements_from_bindings(&bindings);
        assert_eq!(
            statements,
            vec![(
                "http://example.org/s1".to_string(),
                format!("{JANUS_BASELINE_NS}mean"),
                "15".to_string()
            )]
        );
    }

    #[test]
    fn test_last_window_mode_overwrites_previous_window_values() {
        let mut accumulator = HashMap::new();
        super::accumulate_bindings_into_baseline(
            &mut accumulator,
            &[HashMap::from([
                ("sensor".to_string(), "<http://example.org/s1>".to_string()),
                (
                    "mean".to_string(),
                    "\"10\"^^<http://www.w3.org/2001/XMLSchema#decimal>".to_string(),
                ),
            ])],
        );
        accumulator.clear();
        super::accumulate_bindings_into_baseline(
            &mut accumulator,
            &[HashMap::from([
                ("sensor".to_string(), "<http://example.org/s1>".to_string()),
                (
                    "mean".to_string(),
                    "\"30\"^^<http://www.w3.org/2001/XMLSchema#decimal>".to_string(),
                ),
            ])],
        );

        let statements = super::baseline_statements_from_accumulator(&accumulator);
        assert_eq!(
            statements,
            vec![(
                "http://example.org/s1".to_string(),
                format!("{JANUS_BASELINE_NS}mean"),
                "30".to_string()
            )]
        );
    }

    #[test]
    fn test_query_defined_baselines_are_evaluated_over_historical_windows() {
        let config = StreamingConfig {
            segment_base_path: format!(
                "./test_data/janus_api_query_defined_baselines_{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis()
            ),
            ..StreamingConfig::default()
        };
        let storage =
            StreamingSegmentedStorage::new(config).expect("Failed to create segmented storage");

        for i in 1..=10 {
            storage
                .write_rdf(
                    i * 100,
                    "http://example.org/sensor1",
                    "http://example.org/temperature",
                    &(20 + i).to_string(),
                    "http://example.org/sensors",
                )
                .expect("Failed to write RDF event");
        }
        storage.flush().expect("Failed to flush storage");

        let parser = JanusQLParser::new().expect("Failed to create parser");
        let parsed = parser
            .parse(
                r#"
PREFIX ex: <http://example.org/>

FROM NAMED WINDOW ex:liveMinute ON STREAM ex:stream [RANGE 60 STEP 5]
FROM NAMED WINDOW ex:historyDay ON LOG ex:stream [START 0 END 6000]

DEFINE BASELINE ex:dayBaseline ON WINDOW ex:historyDay AS
SELECT ?sensor
       (AVG(?value) AS ?dayAvgValue)
       (COUNT(?value) AS ?dayCount)
WHERE {
  ?sensor ex:temperature ?value .
}
GROUP BY ?sensor

REGISTER RStream ex:output AS
USING BASELINE ex:dayBaseline
SELECT ?sensor
WHERE {
  WINDOW ex:liveMinute {
    ?sensor ex:temperature ?value .
  }
}
GROUP BY ?sensor
                "#,
            )
            .expect("Failed to parse JanusQL query");

        let (_shutdown_tx, shutdown_rx) = mpsc::channel::<()>();
        let bindings =
            collect_query_defined_baseline_bindings(&Arc::new(storage), &parsed, &shutdown_rx)
                .expect("Failed to evaluate query-defined baselines");

        let day_baseline = bindings
            .get("http://example.org/dayBaseline")
            .expect("expected dayBaseline bindings");
        assert_eq!(day_baseline.len(), 1);
        assert!(day_baseline[0].contains_key("sensor"));
        assert!(day_baseline[0].contains_key("dayAvgValue"));
        assert!(day_baseline[0].contains_key("dayCount"));
    }

    #[test]
    fn test_query_defined_baseline_static_graph_can_drive_live_group_by_having() {
        let query = r#"
            PREFIX : <http://example.org/>
            REGISTER RStream :output AS
            SELECT ?sensor
                   (AVG(?value) AS ?minuteAvgValue)
                   ?dayAvgValue
                   ((AVG(?value) - ?dayAvgValue) AS ?difference)
            FROM NAMED WINDOW :liveMinute ON STREAM :stream [RANGE 60 STEP 5]
            WHERE {
                WINDOW :liveMinute {
                    ?sensor :hasValue ?value .
                }
                GRAPH :dayBaseline {
                    ?sensor :dayAvgValue ?dayAvgValue .
                }
            }
            GROUP BY ?sensor ?dayAvgValue
            HAVING(AVG(?value) > ?dayAvgValue)
        "#;
        let definition = BaselineDefinition {
            name: "http://example.org/dayBaseline".to_string(),
            source_window: "http://example.org/historyDay".to_string(),
            raw_query: String::new(),
            select_clause: "SELECT ?sensor (AVG(?value) AS ?dayAvgValue)".to_string(),
            where_clause: "WHERE { ?sensor :hasValue ?value . }".to_string(),
            group_by_clause: Some("GROUP BY ?sensor".to_string()),
            output_variables: vec!["?sensor".to_string(), "?dayAvgValue".to_string()],
        };
        let template = BaselineGraphTemplate {
            baseline_name: "http://example.org/dayBaseline".to_string(),
            triples: vec![TripleTemplate {
                subject: GraphTermTemplate::Variable("sensor".to_string()),
                predicate: GraphTermTemplate::Iri("http://example.org/dayAvgValue".to_string()),
                object: GraphTermTemplate::Variable("dayAvgValue".to_string()),
            }],
        };
        let bindings = vec![HashMap::from([
            ("sensor".to_string(), "<http://example.org/s1>".to_string()),
            (
                "dayAvgValue".to_string(),
                "\"25\"^^<http://www.w3.org/2001/XMLSchema#decimal>".to_string(),
            ),
        ])];
        let prefixes = HashMap::from([("".to_string(), "http://example.org/".to_string())]);

        let quads = materialize_baseline_bindings_as_quads(
            "http://example.org/dayBaseline",
            &definition,
            &template,
            &bindings,
        )
        .expect("baseline quads should materialize");

        let mut processor = LiveStreamProcessing::new(query.to_string()).unwrap();
        processor.register_stream("http://example.org/stream").unwrap();
        for quad in quads {
            processor.add_static_quad(quad);
        }
        processor.start_processing().unwrap();
        processor
            .add_events(
                "http://example.org/stream",
                vec![
                    RDFEvent::new(
                        1,
                        "http://example.org/s1",
                        "http://example.org/hasValue",
                        "30",
                        "",
                    ),
                    RDFEvent::new(
                        2,
                        "http://example.org/s1",
                        "http://example.org/hasValue",
                        "32",
                        "",
                    ),
                ],
            )
            .unwrap();
        processor.close_stream("http://example.org/stream", 100).unwrap();
        thread::sleep(Duration::from_millis(300));

        let results = processor.collect_results(None).unwrap();
        assert!(!results.is_empty(), "expected live result with baseline graph join");
        let rendered = format!("{:?}", results);
        assert!(rendered.contains("dayAvgValue"));
        assert!(rendered.contains("difference"));
    }

    #[test]
    fn test_sliding_query_defined_baseline_rejects_mismatched_step() {
        let parser = JanusQLParser::new().expect("Failed to create parser");
        let parsed = parser
            .parse(
                r#"
PREFIX : <http://example.org/>
FROM NAMED WINDOW :liveMinute ON STREAM :stream [RANGE 60000 STEP 1000]
FROM NAMED WINDOW :sameMinuteYesterday ON LOG :stream [OFFSET 86400000 RANGE 60000 STEP 60000]
DEFINE BASELINE :yesterdayBaseline ON WINDOW :sameMinuteYesterday AS
SELECT ?sensor (AVG(?value) AS ?yesterdayAvgValue)
WHERE {
  ?sensor :hasValue ?value .
}
GROUP BY ?sensor
REGISTER RStream :output AS
USING BASELINE :yesterdayBaseline
SELECT ?sensor ?yesterdayAvgValue
WHERE {
  WINDOW :liveMinute { ?sensor :hasValue ?value . }
  GRAPH :yesterdayBaseline {
    ?sensor :yesterdayAvgValue ?yesterdayAvgValue .
  }
}
                "#,
            )
            .expect("query should parse");

        let err = super::validate_query_defined_baseline_step_alignment(&parsed)
            .expect_err("mismatched steps should be rejected");
        assert!(err.to_string().contains("must match live STEP"));
    }

    #[test]
    fn test_sliding_query_defined_baseline_snapshots_change_with_live_evaluation_time() {
        let config = StreamingConfig {
            segment_base_path: format!(
                "./test_data/janus_api_sliding_baselines_{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis()
            ),
            ..StreamingConfig::default()
        };
        let storage = Arc::new(
            StreamingSegmentedStorage::new(config).expect("Failed to create segmented storage"),
        );

        for (timestamp, value) in [(86_340_002, "10"), (86_400_000, "20")] {
            storage
                .write_rdf(
                    timestamp,
                    "http://example.org/sensor1",
                    "http://example.org/hasValue",
                    value,
                    "http://example.org/history",
                )
                .expect("Failed to write historical RDF event");
        }
        storage.flush().expect("Failed to flush storage");
        for (timestamp, value) in [(86_400_002, "30"), (86_460_000, "50")] {
            storage
                .write_rdf(
                    timestamp,
                    "http://example.org/sensor1",
                    "http://example.org/hasValue",
                    value,
                    "http://example.org/history",
                )
                .expect("Failed to write historical RDF event");
        }
        storage.flush().expect("Failed to flush storage");

        let parser = JanusQLParser::new().expect("Failed to create parser");
        let parsed = parser
            .parse(
                r#"
PREFIX : <http://example.org/>

FROM NAMED WINDOW :liveMinute ON STREAM :stream [RANGE 60000 STEP 60000]
FROM NAMED WINDOW :sameMinuteYesterday ON LOG :stream [OFFSET 86400000 RANGE 60000 STEP 60000]

DEFINE BASELINE :yesterdayBaseline ON WINDOW :sameMinuteYesterday AS
SELECT ?sensor
       (AVG(?value) AS ?yesterdayAvgValue)
WHERE {
  ?sensor :hasValue ?value .
}
GROUP BY ?sensor

REGISTER RStream :output AS
USING BASELINE :yesterdayBaseline
SELECT ?sensor
       (AVG(?value) AS ?currentAvgValue)
       ?yesterdayAvgValue
       ((AVG(?value) - ?yesterdayAvgValue) AS ?difference)
WHERE {
  WINDOW :liveMinute {
    ?sensor :hasValue ?value .
  }
  GRAPH :yesterdayBaseline {
    ?sensor :yesterdayAvgValue ?yesterdayAvgValue .
  }
}
GROUP BY ?sensor ?yesterdayAvgValue
HAVING(AVG(?value) > ?yesterdayAvgValue)
                "#,
            )
            .expect("Failed to parse JanusQL query");

        let baseline_registry = Arc::new(BaselineRegistry::new());
        let latest_rows = Arc::new(RwLock::new(HashMap::new()));
        assert_eq!(
            storage
                .query_rdf(86_340_000, 86_400_000)
                .expect("first historical range should query")
                .len(),
            2
        );
        assert_eq!(
            storage
                .query_rdf(86_400_001, 86_460_001)
                .expect("second historical range should query")
                .len(),
            2
        );
        let definition = parsed
            .ast
            .baseline_definitions
            .iter()
            .find(|definition| definition.name == "http://example.org/yesterdayBaseline")
            .expect("baseline definition should exist");
        let template = parsed
            .baseline_graph_templates
            .iter()
            .find(|template| template.baseline_name == "http://example.org/yesterdayBaseline")
            .expect("baseline graph template should exist");
        let first_snapshot = super::load_or_compute_baseline_snapshot(
            &storage,
            &parsed,
            definition,
            172_800_001,
            &baseline_registry,
        )
        .expect("first baseline snapshot should resolve");
        let second_snapshot = super::load_or_compute_baseline_snapshot(
            &storage,
            &parsed,
            definition,
            172_860_001,
            &baseline_registry,
        )
        .expect("second baseline snapshot should resolve");

        let live_query = r#"
PREFIX : <http://example.org/>
SELECT ?sensor
       (AVG(?value) AS ?currentAvgValue)
       ?yesterdayAvgValue
       ((AVG(?value) - ?yesterdayAvgValue) AS ?difference)
WHERE {
  GRAPH :yesterdayBaseline {
    ?sensor :yesterdayAvgValue ?yesterdayAvgValue .
  }
  GRAPH :liveMinute {
    ?sensor :hasValue ?value .
  }
}
GROUP BY ?sensor ?yesterdayAvgValue
HAVING(AVG(?value) > ?yesterdayAvgValue)
        "#;

        let run_live_evaluation = |events: Vec<(u64, &str)>,
                                   snapshot: &BaselineSnapshot|
         -> HashMap<String, String> {
            super::store_latest_baseline_rows(&latest_rows, snapshot);

            let baseline_quads =
                super::materialize_baseline_snapshot_as_quads(definition, template, snapshot)
                    .expect("baseline quads should materialize");

            let store = Store::new().expect("store should be created");
            for (_timestamp, value) in events {
                store
                    .insert(&Quad::new(
                        NamedNode::new("http://example.org/sensor1").unwrap(),
                        NamedNode::new("http://example.org/hasValue").unwrap(),
                        Literal::new_typed_literal(
                            value,
                            NamedNode::new("http://www.w3.org/2001/XMLSchema#decimal").unwrap(),
                        ),
                        GraphName::NamedNode(
                            NamedNode::new("http://example.org/liveMinute").unwrap(),
                        ),
                    ))
                    .expect("live quad should insert");
            }
            for quad in baseline_quads {
                store.insert(&quad).expect("baseline quad should insert");
            }

            let parsed_query =
                build_evaluator().parse_query(live_query).expect("live SPARQL should parse");
            let results = parsed_query.on_store(&store).execute().expect("query should execute");

            let QueryResults::Solutions(solutions) = results else {
                panic!("expected SELECT solutions");
            };
            let solution = solutions
                .collect::<Result<Vec<_>, _>>()
                .expect("solutions should evaluate")
                .into_iter()
                .next()
                .expect("expected live row joined with baseline snapshot");

            let mut row = HashMap::new();
            for (variable, term) in solution.iter() {
                row.insert(
                    variable.as_str().to_string(),
                    normalize_binding_term(&term.to_string()),
                );
            }
            row
        };

        let first =
            run_live_evaluation(vec![(172_740_002, "25"), (172_800_000, "35")], &first_snapshot);
        assert_eq!(first.get("yesterdayAvgValue"), Some(&"15".to_string()));
        assert_eq!(first.get("difference"), Some(&"15".to_string()));
        assert_eq!(first.get("currentAvgValue"), Some(&"30".to_string()));
        assert_ne!(first.get("currentAvgValue"), Some(&"25".to_string()));

        let second =
            run_live_evaluation(vec![(172_800_002, "60"), (172_860_000, "80")], &second_snapshot);
        assert_eq!(second.get("yesterdayAvgValue"), Some(&"40".to_string()));
        assert_eq!(second.get("difference"), Some(&"30".to_string()));
        assert_eq!(second.get("currentAvgValue"), Some(&"70".to_string()));
        assert_ne!(second.get("yesterdayAvgValue"), Some(&"15".to_string()));

        let first_snapshot = baseline_registry
            .get_snapshot("http://example.org/yesterdayBaseline", 172_800_001)
            .expect("expected snapshot at first evaluation time");
        let second_snapshot = baseline_registry
            .get_snapshot("http://example.org/yesterdayBaseline", 172_860_001)
            .expect("expected snapshot at second evaluation time");
        assert_eq!(first_snapshot.window_start, 86_340_001);
        assert_eq!(first_snapshot.window_end, 86_400_001);
        assert_eq!(second_snapshot.window_start, 86_400_001);
        assert_eq!(second_snapshot.window_end, 86_460_001);
    }
}
