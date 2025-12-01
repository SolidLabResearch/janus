# Janus-QL MVP - Remaining Tasks

This document outlines the remaining work needed to complete the first MVP of Janus, enabling end-to-end hybrid (historical + live) RDF stream processing with JanusQL queries.

## Executive Summary

**Goal:** Send a JanusQL query and receive both historical results (from storage) and live results (from streaming data) as output.

**Current State:** We have all the foundational components (storage, parser, registry, live processing, SPARQL engine, stream bus), but they are not yet wired together for end-to-end query execution.

**Critical Missing Piece:** The `JanusApi::start_query()` method that coordinates historical and live processing.

---

## Current Architecture Status

### ✅ Working Components

| Component | Location | Status | Notes |
|-----------|----------|--------|-------|
| **Storage** | `src/storage/segmented_storage.rs` | ✅ Complete | Dictionary encoding, background flushing, 2.6-3.14M quads/sec write |
| **Parser** | `src/parsing/janusql_parser.rs` | ✅ Complete | Parses JanusQL → RSP-QL + SPARQL queries |
| **Registry** | `src/registry/query_registry.rs` | ✅ Complete | Query registration with metadata |
| **Live Processing** | `src/stream/live_stream_processing.rs` | ✅ Complete | RSP-QL execution via rsp-rs |
| **SPARQL Engine** | `src/querying/oxigraph_adapter.rs` | ✅ Complete | Executes SPARQL on QuadContainer |
| **Stream Bus** | `src/stream_bus/stream_bus.rs` | ✅ Complete | Ingests RDF to storage/brokers |
| **Ingestion CLI** | `src/bin/stream_bus_cli.rs` | ✅ Complete | Command-line data ingestion |

### ⚠️ Partially Implemented

| Component | Location | Status | What's Missing |
|-----------|----------|--------|----------------|
| **JanusApi** | `src/api/janus_api.rs` | ⚠️ Partial | `start_query()` method commented out (lines 128-140) |
| **Result Formatting** | `src/querying/oxigraph_adapter.rs` | ⚠️ Needs work | Returns `Vec<String>` debug format, needs proper bindings |

### ❌ Missing Components

- Query execution coordinator (the heart of `start_query()`)
- Stream bus → live processing integration
- Historical query execution path (storage → SPARQL)
- Query execution CLI or HTTP API
- End-to-end integration tests

---

## Critical Path Tasks (Must Complete for MVP)

### 1. Implement `JanusApi::start_query()` 🔴 HIGH PRIORITY

**File:** `src/api/janus_api.rs` (lines 128-140, currently commented out)

**Signature:**
```rust
pub fn start_query(&self, query_id: &QueryId) -> Result<QueryHandle, JanusApiError>
```

**Implementation Requirements:**

#### 1.1 Query Validation
- Verify query exists in registry via `registry.get(query_id)`
- Check query not already running in `self.running` map
- Increment execution count via `registry.increment_execution_count()`

#### 1.2 Result Channel Setup
```rust
let (result_tx, result_rx) = mpsc::channel::<QueryResult>();
let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();
```

#### 1.3 Spawn Historical Processing Worker
**Thread responsibilities:**
1. Extract historical window time ranges from `metadata.parsed.historical_windows`
2. Query storage: `storage.read_range(start_ts, end_ts)` → `Vec<Event>`
3. Decode events: `Event → RDFEvent` using `Dictionary::decode()`
4. Convert to Oxigraph Quads: `RDFEvent → Quad`
5. Build `QuadContainer` with timestamp
6. Execute SPARQL: `oxigraph_adapter.execute_query(sparql, &container)`
7. Parse results into `QueryResult` with `ResultSource::Historical`
8. Send via `result_tx.send(query_result)`
9. Listen for `shutdown_rx` signal

**Pseudocode:**
```rust
let historical_handle = thread::spawn({
    let storage = Arc::clone(&self.storage);
    let result_tx = result_tx.clone();
    let metadata = metadata.clone();
    
    move || {
        // Extract time range from historical_windows
        for window in metadata.parsed.historical_windows {
            let (start, end) = extract_time_range(&window);
            
            // Query storage
            let events = storage.read_range(start, end).unwrap();
            
            // Decode to RDFEvents
            let rdf_events: Vec<RDFEvent> = events.iter()
                .map(|e| storage.dictionary.decode(e))
                .collect();
            
            // Convert to Quads
            let quads: Vec<Quad> = rdf_events.iter()
                .map(rdf_event_to_quad)
                .collect();
            
            // Execute SPARQL for each window
            for sparql in &metadata.parsed.sparql_queries {
                let container = QuadContainer::new(quads.clone(), end);
                let results = execute_sparql(sparql, &container);
                
                // Send results
                for binding in results {
                    let qr = QueryResult {
                        query_id: metadata.query_id.clone(),
                        timestamp: end,
                        source: ResultSource::Historical,
                        bindings: vec![binding],
                    };
                    result_tx.send(qr).ok();
                }
            }
        }
    }
});
```

#### 1.4 Spawn Live Processing Worker
**Thread responsibilities:**
1. Initialize `LiveStreamProcessing` with RSP-QL query
2. Register streams from `metadata.parsed.live_windows`
3. Start processing via `start_processing()`
4. Subscribe to incoming events (from StreamBus or broker)
5. Add events: `add_event(stream_uri, rdf_event)`
6. Poll results: `collect_results()` or `try_receive_result()`
7. Convert to `QueryResult` with `ResultSource::Live`
8. Send via `result_tx.send(query_result)`
9. Listen for `shutdown_rx` signal

**Pseudocode:**
```rust
let live_handle = thread::spawn({
    let result_tx = result_tx.clone();
    let metadata = metadata.clone();
    
    move || {
        // Initialize live processor
        let mut processor = LiveStreamProcessing::new(
            metadata.parsed.rspql_query.clone()
        ).unwrap();
        
        // Register streams
        for window in &metadata.parsed.live_windows {
            processor.register_stream(&window.stream_uri).unwrap();
        }
        
        processor.start_processing().unwrap();
        
        // Event ingestion loop (needs integration with StreamBus)
        loop {
            // TODO: Receive events from stream source
            // let event = event_receiver.recv()?;
            // processor.add_event(&stream_uri, event)?;
            
            // Poll for results
            if let Some(result) = processor.try_receive_result() {
                let qr = QueryResult {
                    query_id: metadata.query_id.clone(),
                    timestamp: result.timestamp as u64,
                    source: ResultSource::Live,
                    bindings: convert_bindings(result.bindings),
                };
                result_tx.send(qr).ok();
            }
            
            // Check shutdown signal
            if shutdown_rx.try_recv().is_ok() {
                break;
            }
        }
    }
});
```

#### 1.5 Store Running Query State
```rust
let running_query = RunningQuery {
    metadata: metadata.clone(),
    status: Arc::new(RwLock::new(ExecutionStatus::Running)),
    primary_sender: result_tx.clone(),
    subscribers: Vec::new(),
    historical_handle: Some(historical_handle),
    live_handle: Some(live_handle),
    shutdown_sender: vec![shutdown_tx],
};

self.running.lock().unwrap().insert(query_id.clone(), running_query);
```

#### 1.6 Return QueryHandle
```rust
Ok(QueryHandle {
    query_id: query_id.clone(),
    receiver: result_rx,
})
```

**Files to create/modify:**
- `src/api/janus_api.rs` - Implement `start_query()`
- `src/api/helpers.rs` - New file for helper functions:
  - `extract_time_range(window: &WindowDefinition) → (u64, u64)`
  - `rdf_event_to_quad(event: &RDFEvent) → Result<Quad, ...>`
  - `convert_bindings(rsp_bindings: ...) → HashMap<String, String>`
  - `execute_historical_query(...) → Vec<QueryResult>`
  - `execute_live_query(...) → impl Iterator<Item=QueryResult>`

**Estimated complexity:** 🔴 High - 200-300 lines, requires careful threading and error handling

---

### 2. Implement Historical Query Execution Path 🔴 HIGH PRIORITY

**New file:** `src/api/historical_executor.rs`

**Core functionality needed:**

```rust
pub struct HistoricalExecutor {
    storage: Arc<StreamingSegmentedStorage>,
    sparql_engine: OxigraphAdapter,
}

impl HistoricalExecutor {
    pub fn execute_window(
        &self,
        query_id: &QueryId,
        window: &WindowDefinition,
        sparql_query: &str,
    ) -> Result<Vec<QueryResult>, JanusApiError> {
        // 1. Extract time range from window
        let (start_ts, end_ts) = self.extract_time_range(window)?;
        
        // 2. Query storage
        let events = self.storage.read_range(start_ts, end_ts)
            .map_err(|e| JanusApiError::StorageError(e.to_string()))?;
        
        // 3. Decode Event → RDFEvent
        let rdf_events: Vec<RDFEvent> = events.iter()
            .filter_map(|event| self.storage.dictionary.read().unwrap().decode(event).ok())
            .collect();
        
        // 4. Convert RDFEvent → Quad
        let quads: Vec<Quad> = rdf_events.iter()
            .filter_map(|rdf_event| self.rdf_event_to_quad(rdf_event).ok())
            .collect();
        
        // 5. Build QuadContainer
        let container = QuadContainer::new(
            quads.into_iter().collect(),
            end_ts.try_into().unwrap_or(0)
        );
        
        // 6. Execute SPARQL
        let raw_results = self.sparql_engine.execute_query(sparql_query, &container)
            .map_err(|e| JanusApiError::ExecutionError(e.to_string()))?;
        
        // 7. Parse into QueryResult
        let results = raw_results.into_iter()
            .map(|binding_str| {
                QueryResult {
                    query_id: query_id.clone(),
                    timestamp: end_ts,
                    source: ResultSource::Historical,
                    bindings: self.parse_sparql_binding(&binding_str),
                }
            })
            .collect();
        
        Ok(results)
    }
    
    fn extract_time_range(&self, window: &WindowDefinition) -> Result<(u64, u64), JanusApiError> {
        // Parse window.range_ms and window.step_ms
        // For historical: use absolute time ranges or relative to "now"
        todo!()
    }
    
    fn rdf_event_to_quad(&self, event: &RDFEvent) -> Result<Quad, JanusApiError> {
        // Similar to LiveStreamProcessing::rdf_event_to_quad
        let subject = NamedNode::new(&event.subject)
            .map_err(|e| JanusApiError::ExecutionError(format!("Invalid subject: {}", e)))?;
        
        let predicate = NamedNode::new(&event.predicate)
            .map_err(|e| JanusApiError::ExecutionError(format!("Invalid predicate: {}", e)))?;
        
        let object = if event.object.starts_with("http://") || event.object.starts_with("https://") {
            Term::NamedNode(NamedNode::new(&event.object).map_err(|_| 
                JanusApiError::ExecutionError("Invalid object URI".into())
            )?)
        } else {
            Term::Literal(oxigraph::model::Literal::new_simple_literal(&event.object))
        };
        
        let graph = if event.graph.is_empty() || event.graph == "default" {
            GraphName::DefaultGraph
        } else {
            GraphName::NamedNode(NamedNode::new(&event.graph).map_err(|e| 
                JanusApiError::ExecutionError(format!("Invalid graph: {}", e))
            )?)
        };
        
        Ok(Quad::new(subject, predicate, object, graph))
    }
    
    fn parse_sparql_binding(&self, binding_str: &str) -> Vec<HashMap<String, String>> {
        // Parse Oxigraph debug format "{?s: <http://...>, ?p: <http://...>}"
        // Convert to HashMap<String, String>
        // This is a temporary solution until we improve OxigraphAdapter
        todo!()
    }
}
```

**Files to create:**
- `src/api/historical_executor.rs` - New file
- `src/api/mod.rs` - Add `pub mod historical_executor;`

**Estimated complexity:** 🟡 Medium - 150-200 lines

---

### 3. Fix SPARQL Result Format Conversion 🟠 MEDIUM PRIORITY

**Problem:** `OxigraphAdapter::execute_query()` returns `Vec<String>` with debug format like `"{?s: <http://example.org/alice>, ?p: <http://example.org/knows>}"`.

**Solution:** Modify to return structured bindings.

**File:** `src/querying/oxigraph_adapter.rs`

**Changes needed:**

```rust
// Add to trait definition
pub trait SparqlEngine {
    type EngineError: std::error::Error;
    
    // NEW: Return structured bindings instead of strings
    fn execute_query_bindings(
        &self,
        query: &str,
        container: &QuadContainer,
    ) -> Result<Vec<HashMap<String, String>>, Self::EngineError>;
    
    // Keep old method for backward compatibility
    fn execute_query(
        &self,
        query: &str,
        container: &QuadContainer,
    ) -> Result<Vec<String>, Self::EngineError>;
}

// In OxigraphAdapter implementation
impl SparqlEngine for OxigraphAdapter {
    // ... existing code ...
    
    fn execute_query_bindings(
        &self,
        query: &str,
        container: &QuadContainer,
    ) -> Result<Vec<HashMap<String, String>>, Self::EngineError> {
        let store = Store::new()?;
        for quad in &container.elements {
            store.insert(quad)?;
        }
        
        let evaluator = SparqlEvaluator::new();
        let parsed_query = evaluator.parse_query(query)
            .map_err(|e| OxigraphError(e.to_string()))?;
        let results = parsed_query.on_store(&store).execute()?;
        
        let mut bindings_list = Vec::new();
        
        if let QueryResults::Solutions(solutions) = results {
            for solution in solutions {
                let solution = solution?;
                let mut binding = HashMap::new();
                
                for (var, term) in solution.iter() {
                    binding.insert(
                        var.as_str().to_string(),
                        term.to_string()
                    );
                }
                
                bindings_list.push(binding);
            }
        }
        
        Ok(bindings_list)
    }
}
```

**Files to modify:**
- `src/querying/query_processing.rs` - Update `SparqlEngine` trait
- `src/querying/oxigraph_adapter.rs` - Implement `execute_query_bindings()`
- `src/querying/kolibrie_adapter.rs` - Stub implementation

**Estimated complexity:** 🟢 Low - 50-75 lines

---

### 4. Create Stream Bus → Live Processing Integration 🟠 MEDIUM PRIORITY

**Problem:** StreamBus writes to storage/brokers, but doesn't feed LiveStreamProcessing directly.

**Solution Options:**

#### Option A: Event Broadcasting System (Recommended)
Create a pub/sub system where StreamBus publishes events and LiveStreamProcessing subscribes.

**New file:** `src/stream/event_bus.rs`

```rust
use std::sync::{Arc, Mutex, mpsc};
use crate::core::RDFEvent;

pub struct EventBus {
    subscribers: Arc<Mutex<Vec<mpsc::Sender<RDFEvent>>>>,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            subscribers: Arc::new(Mutex::new(Vec::new())),
        }
    }
    
    pub fn subscribe(&self) -> mpsc::Receiver<RDFEvent> {
        let (tx, rx) = mpsc::channel();
        self.subscribers.lock().unwrap().push(tx);
        rx
    }
    
    pub fn publish(&self, event: RDFEvent) {
        let subscribers = self.subscribers.lock().unwrap();
        for tx in subscribers.iter() {
            tx.send(event.clone()).ok(); // Ignore disconnected subscribers
        }
    }
}
```

**Modify StreamBus:**
```rust
// In src/stream_bus/stream_bus.rs
pub struct StreamBusConfig {
    // ... existing fields ...
    pub event_bus: Option<Arc<EventBus>>, // NEW
}

// In process_line method
if let Some(ref event_bus) = self.config.event_bus {
    event_bus.publish(rdf_event.clone());
}
```

**Modify JanusApi::start_query() live worker:**
```rust
let event_receiver = event_bus.subscribe();

loop {
    if let Ok(event) = event_receiver.try_recv() {
        // Determine which stream this event belongs to
        let stream_uri = determine_stream_uri(&event);
        processor.add_event(&stream_uri, event)?;
    }
    
    // Poll for results...
}
```

#### Option B: Direct Integration
Add callback to StreamBus that directly calls LiveStreamProcessing.

**Files to create/modify:**
- `src/stream/event_bus.rs` - New event broadcasting system
- `src/stream_bus/stream_bus.rs` - Add event_bus field to config
- `src/api/janus_api.rs` - Subscribe live worker to event bus
- `src/stream/mod.rs` - Export EventBus

**Estimated complexity:** 🟡 Medium - 100-150 lines

---

### 5. Create End-to-End Integration Test 🟠 MEDIUM PRIORITY

**New file:** `tests/mvp_integration_test.rs`

**Test scenario:**
1. Create storage, parser, registry, API
2. Register a JanusQL query with historical and live windows
3. Ingest historical data via StreamBus
4. Start query execution
5. Receive and verify historical results
6. Ingest live data via StreamBus
7. Receive and verify live results
8. Stop query execution

```rust
#[test]
fn test_end_to_end_hybrid_query() {
    // Setup
    let temp_dir = tempdir().unwrap();
    let storage_path = temp_dir.path().join("storage");
    
    let storage = Arc::new(
        StreamingSegmentedStorage::new(&storage_path, StreamingConfig::default()).unwrap()
    );
    
    let parser = JanusQLParser::new();
    let registry = Arc::new(QueryRegistry::new());
    let api = JanusApi::new(parser, registry, Arc::clone(&storage)).unwrap();
    
    // Register query
    let query = r#"
        PREFIX ex: <http://example.org/>
        REGISTER RStream <output> AS
        SELECT ?sensor ?temp
        FROM NAMED WINDOW ex:historical ON STREAM ex:sensors [RANGE 60000 STEP 10000]
        FROM NAMED WINDOW ex:live ON STREAM ex:sensors [RANGE 5000 STEP 1000]
        WHERE {
            WINDOW ?w { ?sensor ex:temperature ?temp }
        }
    "#;
    
    api.register_query("q1".to_string(), query).unwrap();
    
    // Ingest historical data
    let historical_events = vec![
        RDFEvent::new(1000, "http://ex.org/s1", "http://ex.org/temp", "23.5", "http://ex.org/g1"),
        RDFEvent::new(2000, "http://ex.org/s2", "http://ex.org/temp", "24.1", "http://ex.org/g1"),
    ];
    
    for event in historical_events {
        storage.write(&[event]).unwrap();
    }
    
    // Flush storage
    std::thread::sleep(Duration::from_millis(100));
    
    // Start query
    let handle = api.start_query(&"q1".to_string()).unwrap();
    
    // Receive historical results
    let mut historical_count = 0;
    for _ in 0..10 {
        if let Some(result) = handle.try_receive() {
            assert_eq!(result.source, ResultSource::Historical);
            historical_count += 1;
        } else {
            break;
        }
    }
    assert!(historical_count > 0, "Should receive historical results");
    
    // Ingest live data (via StreamBus with EventBus integration)
    let event_bus = Arc::new(EventBus::new());
    let live_event = RDFEvent::new(
        current_timestamp(),
        "http://ex.org/s3",
        "http://ex.org/temp",
        "25.0",
        "http://ex.org/g1"
    );
    event_bus.publish(live_event);
    
    // Receive live results
    std::thread::sleep(Duration::from_millis(100));
    let mut live_count = 0;
    for _ in 0..10 {
        if let Some(result) = handle.try_receive() {
            if result.source == ResultSource::Live {
                live_count += 1;
            }
        } else {
            break;
        }
    }
    assert!(live_count > 0, "Should receive live results");
}
```

**Files to create:**
- `tests/mvp_integration_test.rs` - New comprehensive integration test

**Estimated complexity:** 🟡 Medium - 150-200 lines

---

### 6. Implement `stop_query()` Method 🟢 LOW PRIORITY

**File:** `src/api/janus_api.rs`

```rust
pub fn stop_query(&self, query_id: &QueryId) -> Result<(), JanusApiError> {
    let mut running_map = self.running.lock().unwrap();
    
    if let Some(mut running_query) = running_map.remove(query_id) {
        // Update status
        *running_query.status.write().unwrap() = ExecutionStatus::Stopped;
        
        // Send shutdown signals
        for tx in running_query.shutdown_sender {
            tx.send(()).ok();
        }
        
        // Join threads
        if let Some(handle) = running_query.historical_handle.take() {
            handle.join().ok();
        }
        if let Some(handle) = running_query.live_handle.take() {
            handle.join().ok();
        }
        
        Ok(())
    } else {
        Err(JanusApiError::RegistryError("Query not running".into()))
    }
}
```

**Estimated complexity:** 🟢 Low - 30-50 lines

---

## Important Tasks (Needed for Usability)

### 7. Create Query Execution CLI 🟠 MEDIUM PRIORITY

**New file:** `src/bin/query_cli.rs`

**Features:**
- Register queries from file or stdin
- Start query execution
- Stream results to stdout (JSON or table format)
- Stop query on Ctrl+C

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "query_cli")]
#[command(about = "Janus Query Execution CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Register a new query
    Register {
        /// Query ID
        #[arg(short, long)]
        id: String,
        
        /// JanusQL query file
        #[arg(short, long)]
        query_file: String,
    },
    
    /// Execute a registered query
    Execute {
        /// Query ID
        #[arg(short, long)]
        id: String,
        
        /// Output format (json|table)
        #[arg(short, long, default_value = "table")]
        format: String,
        
        /// Maximum results to display (0 = unlimited)
        #[arg(short, long, default_value = "0")]
        limit: usize,
    },
    
    /// List all registered queries
    List,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    
    // Initialize API (would need config file for storage path, etc.)
    let api = create_api()?;
    
    match cli.command {
        Commands::Register { id, query_file } => {
            let query = std::fs::read_to_string(query_file)?;
            let metadata = api.register_query(id, &query)?;
            println!("Registered query: {:?}", metadata);
        }
        
        Commands::Execute { id, format, limit } => {
            let handle = api.start_query(&id)?;
            
            println!("Executing query '{}'...", id);
            println!("Press Ctrl+C to stop\n");
            
            let mut count = 0;
            loop {
                if let Some(result) = handle.receive() {
                    match format.as_str() {
                        "json" => println!("{}", serde_json::to_string(&result)?),
                        "table" => print_table_row(&result),
                        _ => eprintln!("Unknown format"),
                    }
                    
                    count += 1;
                    if limit > 0 && count >= limit {
                        break;
                    }
                }
            }
        }
        
        Commands::List => {
            // List all queries
            todo!()
        }
    }
    
    Ok(())
}
```

**Files to create:**
- `src/bin/query_cli.rs` - New CLI binary
- Update `Cargo.toml` to add `query_cli` to `[[bin]]` section

**Estimated complexity:** 🟡 Medium - 200-250 lines

---

### 8. Add Configuration File Support 🟢 LOW PRIORITY

**New file:** `janus_config.toml` (example)

```toml
[storage]
path = "./data/janus_storage"
max_batch_size_bytes = 10485760
flush_interval_ms = 5000

[registry]
max_queries = 100

[brokers.kafka]
enabled = false
bootstrap_servers = "localhost:9092"

[brokers.mqtt]
enabled = false
broker_url = "tcp://localhost:1883"

[api]
mode = "cli"  # or "http"

[api.http]
enabled = false
host = "127.0.0.1"
port = 8080

[api.websocket]
enabled = false
port = 8081
```

**New file:** `src/config.rs`

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct JanusConfig {
    pub storage: StorageConfig,
    pub registry: RegistryConfig,
    pub brokers: BrokersConfig,
    pub api: ApiConfig,
}

impl JanusConfig {
    pub fn from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let config: JanusConfig = toml::from_str(&content)?;
        Ok(config)
    }
}
```

**Estimated complexity:** 🟢 Low - 100-150 lines

---

## Nice-to-Have Tasks (Future Enhancements)

### 9. HTTP + WebSocket API Server 🔵 OPTIONAL

**For Flutter dashboard integration**

**New file:** `src/bin/janus_server.rs`

```rust
use axum::{
    extract::{State, Path, WebSocketUpgrade},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::cors::CorsLayer;

#[derive(Clone)]
struct AppState {
    api: Arc<JanusApi>,
}

#[tokio::main]
async fn main() {
    let api = Arc::new(create_api().unwrap());
    let state = AppState { api };
    
    let app = Router::new()
        .route("/api/queries", post(register_query))
        .route("/api/queries", get(list_queries))
        .route("/api/queries/:id/start", post(start_query))
        .route("/api/queries/:id/stop", post(stop_query))
        .route("/api/queries/:id/results", get(query_results_ws))
        .layer(CorsLayer::permissive())
        .with_state(state);
    
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080")
        .await
        .unwrap();
    
    println!("Janus server listening on http://127.0.0.1:8080");
    axum::serve(listener, app).await.unwrap();
}

async fn register_query(
    State(state): State<AppState>,
    Json(payload): Json<RegisterRequest>,
) -> impl IntoResponse {
    // Implementation
}

async fn query_results_ws(
    ws: WebSocketUpgrade,
    Path(query_id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_results_socket(socket, query_id, state))
}

async fn handle_results_socket(
    mut socket: WebSocket,
    query_id: String,
    state: AppState,
) {
    // Start query and stream results over WebSocket
}
```

**Dependencies to add to Cargo.toml:**
```toml
axum = "0.7"
tokio = { version = "1", features = ["full"] }
tower-http = { version = "0.5", features = ["cors"] }
```

**Estimated complexity:** 🔴 High - 300-400 lines

---

### 10. Docker Compose for Local Testing 🟢 LOW PRIORITY

**New file:** `docker-compose.yml`

```yaml
version: '3.8'

services:
  kafka:
    image: confluentinc/cp-kafka:7.5.0
    ports:
      - "9092:9092"
    environment:
      KAFKA_BROKER_ID: 1
      KAFKA_ZOOKEEPER_CONNECT: zookeeper:2181
      KAFKA_ADVERTISED_LISTENERS: PLAINTEXT://localhost:9092
      KAFKA_OFFSETS_TOPIC_REPLICATION_FACTOR: 1
    depends_on:
      - zookeeper

  zookeeper:
    image: confluentinc/cp-zookeeper:7.5.0
    ports:
      - "2181:2181"
    environment:
      ZOOKEEPER_CLIENT_PORT: 2181

  mosquitto:
    image: eclipse-mosquitto:2
    ports:
      - "1883:1883"
      - "9001:9001"
    volumes:
      - ./mosquitto.conf:/mosquitto/config/mosquitto.conf

  janus:
    build: .
    ports:
      - "8080:8080"
    depends_on:
      - kafka
      - mosquitto
    volumes:
      - ./data:/data
    environment:
      STORAGE_PATH: /data/janus_storage
      KAFKA_BROKERS: kafka:9092
      MQTT_BROKER: mosquitto:1883
```

**Estimated complexity:** 🟢 Low - 50-75 lines

---

### 11. Production Monitoring & Logging 🔵 OPTIONAL

**Add structured logging with tracing:**

```rust
use tracing::{info, warn, error, debug};
use tracing_subscriber;

// In main or server initialization
tracing_subscriber::fmt::init();

// In JanusApi::start_query()
info!(query_id = %query_id, "Starting query execution");

// In workers
debug!(query_id = %query_id, events_processed = count, "Historical processing progress");
warn!(query_id = %query_id, error = %e, "Failed to decode event");
```

**Add metrics:**
- Query execution count
- Average response time
- Historical vs live result ratio
- Error rates
- Memory usage

**Estimated complexity:** 🟡 Medium - 150-200 lines

---

## Testing Strategy

### Unit Tests
- [x] Storage tests (existing)
- [x] Parser tests (existing)
- [x] Registry tests (existing)
- [x] StreamBus tests (existing)
- [ ] HistoricalExecutor tests
- [ ] EventBus tests
- [ ] Result format conversion tests

### Integration Tests
- [x] StreamBus CLI tests (existing)
- [ ] End-to-end MVP test (Task #5)
- [ ] Multi-query execution test
- [ ] Historical-only query test
- [ ] Live-only query test
- [ ] Error handling tests

### Performance Tests
- [ ] Concurrent query execution
- [ ] Large historical window queries
- [ ] High-throughput live stream processing
- [ ] Memory usage under load

---

## Timeline Estimate

### Phase 1: Core MVP (1-2 weeks)
- **Day 1-3:** Implement `JanusApi::start_query()` skeleton + historical executor
- **Day 4-5:** Fix SPARQL result formatting
- **Day 6-7:** Implement EventBus integration
- **Day 8-10:** End-to-end integration test + debugging
- **Day 11-14:** Query CLI + documentation

### Phase 2: Refinement (1 week)
- **Day 15-17:** Error handling, graceful shutdown, edge cases
- **Day 18-21:** Performance testing, optimization, bug fixes

### Phase 3: Production Ready (1-2 weeks, optional)
- **Week 4:** HTTP/WebSocket API
- **Week 5:** Docker Compose, monitoring, deployment docs

---

## Success Criteria

The MVP is complete when:

1. ✅ A user can register a JanusQL query via CLI
2. ✅ The query specifies both historical and live windows
3. ✅ Historical data is ingested via `stream_bus_cli`
4. ✅ Query execution returns historical results first
5. ✅ Live data is ingested in real-time
6. ✅ Query execution returns live results as data arrives
7. ✅ Results clearly distinguish historical vs live source
8. ✅ Query can be stopped gracefully
9. ✅ All integration tests pass
10. ✅ Documentation is complete

---

## Getting Started

**Recommended order of implementation:**

1. Start with Task #3 (SPARQL result formatting) - smallest, enables others
2. Then Task #2 (Historical executor) - foundational for historical path
3. Then Task #4 (EventBus) - enables live processing integration
4. Then Task #1 (start_query implementation) - ties everything together
5. Then Task #5 (Integration test) - validates the whole flow
6. Then Task #7 (Query CLI) - makes it usable
7. Everything else can follow based on priority

**Development workflow:**
```bash
# 1. Create feature branch
git checkout -b feature/mvp-start-query

# 2. Implement tasks incrementally with tests
cargo test --test <test_name>

# 3. Run full test suite
make test

# 4. Format and lint
make fmt
make clippy

# 5. Commit and push
git commit -m "feat: implement JanusApi::start_query() [Task #1]"
git push origin feature/mvp-start-query
```

---

## Questions & Decisions Needed

1. **Time range specification:** How should users specify historical time ranges in JanusQL?
   - Option A: Absolute timestamps (e.g., `RANGE 1640000000000 TO 1640003600000`)
   - Option B: Relative to query start (e.g., `RANGE LAST 1 HOUR`)
   - Option C: Both supported

2. **Event routing:** How to determine which stream an event belongs to for live processing?
   - Option A: Use graph URI as stream identifier
   - Option B: Add explicit stream metadata to RDFEvent
   - Option C: Configure stream-to-graph mapping in query

3. **Result delivery:** Should QueryHandle support multiple subscribers?
   - Current design has `primary_sender` + `subscribers` list but not implemented
   - Is this needed for MVP?

4. **Error handling:** What should happen if historical processing fails but live succeeds?
   - Option A: Continue with live results, log error
   - Option B: Fail entire query
   - Option C: Send partial results with error flag

5. **Persistence:** Should running queries persist across restarts?
   - Probably not for MVP, but worth considering architecture

---

## References

- **Architecture:** `ARCHITECTURE.md`
- **Benchmarks:** `BENCHMARK_RESULTS.md`
- **Copilot Instructions:** `.github/copilot-instructions.md`
- **Stream Bus Docs:** `docs/STREAM_BUS_CLI.md`
- **Core modules:**
  - Storage: `src/storage/segmented_storage.rs`
  - Parser: `src/parsing/janusql_parser.rs`
  - Live: `src/stream/live_stream_processing.rs`
  - SPARQL: `src/querying/oxigraph_adapter.rs`
