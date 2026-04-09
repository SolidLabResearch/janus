use crate::{
    core::RDFEvent,
    execution::{HistoricalExecutor, ResultConverter},
    parsing::janusql_parser::{JanusQLParser, WindowType},
    querying::oxigraph_adapter::OxigraphAdapter,
    registry::query_registry::{BaselineBootstrapMode, QueryId, QueryMetadata, QueryRegistry},
    storage::segmented_storage::StreamingSegmentedStorage,
    stream::{
        live_stream_processing::LiveStreamProcessing,
        mqtt_subscriber::{MqttSubscriber, MqttSubscriberConfig},
    },
};
use std::{
    cmp::Ordering,
    collections::HashMap,
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
    // Primary sender used to send the results to the main subscriber
    primary_sender: Sender<QueryResult>,
    // Additional senders for other subscribers (if any)
    subscribers: Vec<Sender<QueryResult>>,
    // thread handles for historical and live workers
    historical_handles: Vec<thread::JoinHandle<()>>,
    baseline_handle: Option<thread::JoinHandle<()>>,
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
        let mut historical_handles = Vec::new();
        let mut shutdown_senders = Vec::new();
        let status = Arc::new(RwLock::new(
            if !parsed.live_windows.is_empty() && !parsed.historical_windows.is_empty() {
                ExecutionStatus::WarmingBaseline
            } else {
                ExecutionStatus::Running
            },
        ));

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
        let mut baseline_handle = None;

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

            if !parsed.historical_windows.is_empty() {
                let storage = Arc::clone(&self.storage);
                let parsed_clone = parsed.clone();
                let processor_for_baseline = Arc::clone(&live_processor);
                let status_for_baseline = Arc::clone(&status);
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
                            }
                            Err(err) => {
                                eprintln!("Async baseline warm-up error: {}", err);
                                if let Ok(mut state) = status_for_baseline.write() {
                                    *state = ExecutionStatus::Failed(err.to_string());
                                }
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
                mqtt_subscriber_handle = Some(sub_handle);
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

        // 6. Store running query information
        let running = RunningQuery {
            metadata,
            status,
            primary_sender: result_tx,
            subscribers: vec![],
            historical_handles,
            baseline_handle,
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

        let sparql_query = parsed.sparql_queries.get(index).ok_or_else(|| {
            JanusApiError::ExecutionError(format!(
                "Missing SPARQL query for historical window {}",
                index
            ))
        })?;

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

#[cfg(test)]
fn materialize_bindings_as_static_baseline(
    processor: &mut LiveStreamProcessing,
    bindings: &[HashMap<String, String>],
) -> Result<(), JanusApiError> {
    let statements = baseline_statements_from_bindings(bindings);
    materialize_static_baseline_statements(processor, &statements)
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

            entry.last_value = normalized.clone();
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
        baseline_statements_from_bindings, materialize_bindings_as_static_baseline,
        normalize_binding_term, parse_mqtt_uri, JANUS_BASELINE_NS,
    };
    use crate::{core::RDFEvent, stream::live_stream_processing::LiveStreamProcessing};
    use std::{collections::HashMap, thread, time::Duration};

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
}
