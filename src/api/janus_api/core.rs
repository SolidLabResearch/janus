use crate::{
    api::janus_api::{
        baseline::{
            build_query_defined_baseline_provider, collect_query_baseline_statements,
            initialize_fixed_query_defined_baselines, materialize_static_baseline_statements,
        },
        mqtt::parse_mqtt_uri,
        types::{ExecutionStatus, JanusApiError, QueryHandle, QueryResult, RunningQuery},
        validation::{
            validate_query_defined_baseline_access, validate_query_defined_baseline_step_alignment,
        },
    },
    execution::{HistoricalExecutor, ResultConverter},
    parsing::janusql_parser::{JanusQLParser, WindowType},
    querying::oxigraph_adapter::OxigraphAdapter,
    registry::{
        baseline_registry::BaselineRegistry,
        query_registry::{BaselineBootstrapMode, QueryId, QueryMetadata, QueryRegistry},
    },
    storage::segmented_storage::StreamingSegmentedStorage,
    stream::{
        live_stream_processing::LiveStreamProcessing,
        mqtt_subscriber::{MqttSubscriber, MqttSubscriberConfig},
    },
};
use std::{
    collections::HashMap,
    sync::{mpsc, Arc, Mutex, RwLock},
    thread,
};

/// Top-level API which coordinates the registry, the historical storage of data, and the live processing of data streams.
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

    /// Register a JanusQL Query within the Query Registry.
    /// It just stores the query without executing it.
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
                    if let Err(e) = processor.register_stream(&window.source_name) {
                        eprintln!("Failed to register stream '{}': {}", window.source_name, e);
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
                let (host, port, topic) = parse_mqtt_uri(&window.source_name);

                let config = MqttSubscriberConfig {
                    host,
                    port,
                    client_id: format!("janus_live_{}_{}", query_id.clone(), window.source_name),
                    keep_alive_secs: 30,
                    topic,
                    stream_uri: window.source_name.clone(),
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
