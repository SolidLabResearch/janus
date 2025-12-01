# Janus MVP Quick Start Implementation Guide

## TL;DR - What You Need to Do

You asked: *"What is left to be done so that I can send a Janus-QL Query for the first MVP so that the historical and live processing is done and the results are returned as an output?"*

**Answer:** Implement 4 critical missing pieces (in this order):

1. **Fix SPARQL result format** (~1 hour)
2. **Create historical query executor** (~4 hours)
3. **Create event bus for live integration** (~3 hours)
4. **Wire it all together in `start_query()`** (~6 hours)

Then add a CLI and test (another ~6 hours). **Total: ~20 hours of focused work.**

---

## What You Already Have (✅ Working)

| Component | Status | What It Does |
|-----------|--------|--------------|
| **Storage** | ✅ Complete | Stores 2.6M+ quads/sec, dictionary-encoded, background flush |
| **Parser** | ✅ Complete | Parses JanusQL → RSP-QL + SPARQL queries |
| **Registry** | ✅ Complete | Registers queries with metadata |
| **Live Processing** | ✅ Complete | RSP-QL execution via rsp-rs engine |
| **SPARQL Engine** | ✅ Complete | Executes SPARQL on quads (but format needs fix) |
| **Stream Bus** | ✅ Complete | Ingests RDF to storage/brokers |
| **Ingestion CLI** | ✅ Complete | `stream_bus_cli` for data ingestion |

---

## What's Missing (❌ Gaps)

```
┌─────────────────────────────────────────────┐
│  User sends JanusQL query                   │
│  "Show me temp readings from last hour      │
│   AND keep showing live updates"            │
└──────────────────┬──────────────────────────┘
                   │
                   ▼
         ┌─────────────────────┐
         │  JanusApi           │
         │  register_query() ✅│
         │  start_query()   ❌ │ <-- MISSING!
         └─────────────────────┘
                   │
        ┌──────────┴──────────┐
        │                     │
        ▼                     ▼
   Historical Path        Live Path
        ❌                   ❌
        │                     │
   Need executor         Need event bus
   to query storage      to feed live engine
```

---

## Implementation Roadmap

### Task 1: Fix SPARQL Result Format (1 hour) 🟢

**Why:** OxigraphAdapter returns `Vec<String>` with debug format. Need structured bindings.

**File:** `src/querying/oxigraph_adapter.rs`

**Add this method:**

```rust
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
```

**Test it:**
```bash
cargo test --test integration_tests oxigraph
```

---

### Task 2: Create Historical Executor (4 hours) 🟡

**Why:** Need to query storage and execute SPARQL for historical windows.

**File:** `src/api/historical_executor.rs` (new file)

**Implementation:**

```rust
use crate::{
    api::janus_api::{JanusApiError, QueryResult, ResultSource},
    core::RDFEvent,
    parsing::janusql_parser::WindowDefinition,
    querying::oxigraph_adapter::OxigraphAdapter,
    registry::query_registry::QueryId,
    storage::segmented_storage::StreamingSegmentedStorage,
};
use oxigraph::model::{GraphName, NamedNode, Quad, Term};
use rsp_rs::QuadContainer;
use std::{collections::HashMap, sync::Arc};

pub struct HistoricalExecutor {
    storage: Arc<StreamingSegmentedStorage>,
}

impl HistoricalExecutor {
    pub fn new(storage: Arc<StreamingSegmentedStorage>) -> Self {
        Self { storage }
    }
    
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
            .filter_map(|event| {
                self.storage.dictionary.read().ok()
                    .and_then(|dict| dict.decode(event).ok())
            })
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
        let adapter = OxigraphAdapter::new();
        let bindings = adapter.execute_query_bindings(sparql_query, &container)
            .map_err(|e| JanusApiError::ExecutionError(e.to_string()))?;
        
        // 7. Convert to QueryResult
        let results = bindings.into_iter()
            .map(|binding| QueryResult {
                query_id: query_id.clone(),
                timestamp: end_ts,
                source: ResultSource::Historical,
                bindings: vec![binding],
            })
            .collect();
        
        Ok(results)
    }
    
    fn extract_time_range(&self, window: &WindowDefinition) 
        -> Result<(u64, u64), JanusApiError> {
        // For MVP: use current time - range_ms as start
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        let end_ts = now;
        let start_ts = now.saturating_sub(window.range_ms);
        
        Ok((start_ts, end_ts))
    }
    
    fn rdf_event_to_quad(&self, event: &RDFEvent) 
        -> Result<Quad, JanusApiError> {
        let subject = NamedNode::new(&event.subject)
            .map_err(|e| JanusApiError::ExecutionError(
                format!("Invalid subject: {}", e)
            ))?;
        
        let predicate = NamedNode::new(&event.predicate)
            .map_err(|e| JanusApiError::ExecutionError(
                format!("Invalid predicate: {}", e)
            ))?;
        
        let object = if event.object.starts_with("http://") || 
                        event.object.starts_with("https://") {
            Term::NamedNode(NamedNode::new(&event.object)
                .map_err(|_| JanusApiError::ExecutionError(
                    "Invalid object URI".into()
                ))?)
        } else {
            Term::Literal(oxigraph::model::Literal::new_simple_literal(
                &event.object
            ))
        };
        
        let graph = if event.graph.is_empty() || event.graph == "default" {
            GraphName::DefaultGraph
        } else {
            GraphName::NamedNode(NamedNode::new(&event.graph)
                .map_err(|e| JanusApiError::ExecutionError(
                    format!("Invalid graph: {}", e)
                ))?)
        };
        
        Ok(Quad::new(subject, predicate, object, graph))
    }
}
```

**Add to `src/api/mod.rs`:**
```rust
pub mod historical_executor;
```

**Test it:**
```bash
cargo test --lib historical_executor
```

---

### Task 3: Create Event Bus (3 hours) 🟡

**Why:** Need to broadcast events from StreamBus to LiveStreamProcessing.

**File:** `src/stream/event_bus.rs` (new file)

**Implementation:**

```rust
use crate::core::RDFEvent;
use std::sync::{mpsc, Arc, Mutex};

/// Event broadcasting system for live stream processing
pub struct EventBus {
    subscribers: Arc<Mutex<Vec<mpsc::Sender<RDFEvent>>>>,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            subscribers: Arc::new(Mutex::new(Vec::new())),
        }
    }
    
    /// Subscribe to events. Returns a receiver channel.
    pub fn subscribe(&self) -> mpsc::Receiver<RDFEvent> {
        let (tx, rx) = mpsc::channel();
        self.subscribers.lock().unwrap().push(tx);
        rx
    }
    
    /// Publish an event to all subscribers.
    pub fn publish(&self, event: RDFEvent) {
        let mut subscribers = self.subscribers.lock().unwrap();
        
        // Remove disconnected subscribers
        subscribers.retain(|tx| tx.send(event.clone()).is_ok());
    }
    
    /// Get current subscriber count
    pub fn subscriber_count(&self) -> usize {
        self.subscribers.lock().unwrap().len()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    
    #[test]
    fn test_event_bus_publish_subscribe() {
        let bus = EventBus::new();
        let rx = bus.subscribe();
        
        let event = RDFEvent::new(
            1000,
            "http://ex.org/s",
            "http://ex.org/p",
            "o",
            "http://ex.org/g"
        );
        
        bus.publish(event.clone());
        
        let received = rx.recv_timeout(Duration::from_millis(100)).unwrap();
        assert_eq!(received.subject, event.subject);
    }
}
```

**Add to `src/stream/mod.rs`:**
```rust
pub mod event_bus;
pub use event_bus::EventBus;
```

**Integrate with StreamBus** in `src/stream_bus/stream_bus.rs`:

```rust
// Add to StreamBusConfig
pub struct StreamBusConfig {
    // ... existing fields ...
    pub event_bus: Option<Arc<EventBus>>,
}

// In process_line() method, after writing to storage:
if let Some(ref event_bus) = self.config.event_bus {
    event_bus.publish(rdf_event.clone());
}
```

**Test it:**
```bash
cargo test --lib event_bus
```

---

### Task 4: Wire Everything in `start_query()` (6 hours) 🔴

**Why:** This is the coordinator that makes it all work.

**File:** `src/api/janus_api.rs`

**Uncomment and implement lines 128-140:**

```rust
pub fn start_query(&self, query_id: &QueryId) -> Result<QueryHandle, JanusApiError> {
    // 1. Validate query exists
    let metadata = self.registry.get(query_id).ok_or_else(|| 
        JanusApiError::RegistryError("Query not found".into())
    )?;
    
    // 2. Check not already running
    {
        let running_map = self.running.lock().unwrap();
        if running_map.contains_key(query_id) {
            return Err(JanusApiError::ExecutionError(
                "Query already running".into()
            ));
        }
    }
    
    // 3. Create channels
    let (result_tx, result_rx) = mpsc::channel::<QueryResult>();
    let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();
    
    // 4. Spawn historical worker
    let historical_handle = {
        let storage = Arc::clone(&self.storage);
        let metadata = metadata.clone();
        let result_tx = result_tx.clone();
        let shutdown_rx_clone = shutdown_rx;
        
        std::thread::spawn(move || {
            let executor = HistoricalExecutor::new(storage);
            
            for window in &metadata.parsed.historical_windows {
                // Check shutdown signal
                if shutdown_rx_clone.try_recv().is_ok() {
                    break;
                }
                
                for sparql in &metadata.parsed.sparql_queries {
                    match executor.execute_window(
                        &metadata.query_id, 
                        window, 
                        sparql
                    ) {
                        Ok(results) => {
                            for result in results {
                                result_tx.send(result).ok();
                            }
                        }
                        Err(e) => eprintln!("Historical error: {}", e),
                    }
                }
            }
        })
    };
    
    // 5. Spawn live worker
    let live_handle = {
        let metadata = metadata.clone();
        let result_tx = result_tx.clone();
        
        std::thread::spawn(move || {
            // Initialize live processor
            let mut processor = match LiveStreamProcessing::new(
                metadata.parsed.rspql_query.clone()
            ) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("Failed to init live processor: {}", e);
                    return;
                }
            };
            
            // Register streams
            for window in &metadata.parsed.live_windows {
                if let Err(e) = processor.register_stream(&window.stream_uri) {
                    eprintln!("Failed to register stream: {}", e);
                }
            }
            
            // Start processing
            if let Err(e) = processor.start_processing() {
                eprintln!("Failed to start processing: {}", e);
                return;
            }
            
            // TODO: Subscribe to EventBus here
            // For MVP, this will be added after EventBus integration
            
            // Poll for results
            loop {
                if let Some(result) = processor.try_receive_result() {
                    let qr = QueryResult {
                        query_id: metadata.query_id.clone(),
                        timestamp: result.timestamp as u64,
                        source: ResultSource::Live,
                        bindings: vec![result.bindings.into_iter()
                            .map(|(k, v)| (k, v.to_string()))
                            .collect()],
                    };
                    result_tx.send(qr).ok();
                }
                
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        })
    };
    
    // 6. Store running query
    {
        let running_query = RunningQuery {
            metadata: metadata.clone(),
            status: Arc::new(RwLock::new(ExecutionStatus::Running)),
            primary_sender: result_tx.clone(),
            subscribers: Vec::new(),
            historical_handle: Some(historical_handle),
            live_handle: Some(live_handle),
            shutdown_sender: vec![shutdown_tx],
        };
        
        self.running.lock().unwrap().insert(
            query_id.clone(), 
            running_query
        );
    }
    
    // 7. Increment execution count
    self.registry.increment_execution_count(query_id).ok();
    
    // 8. Return handle
    Ok(QueryHandle {
        query_id: query_id.clone(),
        receiver: result_rx,
    })
}
```

**Test it:**
```bash
cargo test --lib janus_api
```

---

### Task 5: Create Query CLI (4 hours) 🟡

**File:** `src/bin/query_cli.rs` (new file)

**Basic implementation:**

```rust
use clap::{Parser, Subcommand};
use janus::{
    api::janus_api::JanusApi,
    parsing::janusql_parser::JanusQLParser,
    registry::query_registry::QueryRegistry,
    storage::{segmented_storage::StreamingSegmentedStorage, util::StreamingConfig},
};
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "query_cli")]
#[command(about = "Janus Query Execution CLI")]
struct Cli {
    /// Storage path
    #[arg(short, long, default_value = "./data/janus_storage")]
    storage: String,
    
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Register {
        #[arg(short, long)]
        id: String,
        
        #[arg(short, long)]
        query_file: String,
    },
    
    Execute {
        #[arg(short, long)]
        id: String,
        
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    
    // Initialize components
    let storage = Arc::new(StreamingSegmentedStorage::new(
        &cli.storage,
        StreamingConfig::default(),
    )?);
    
    let parser = JanusQLParser::new();
    let registry = Arc::new(QueryRegistry::new());
    let api = JanusApi::new(parser, registry, storage)?;
    
    match cli.command {
        Commands::Register { id, query_file } => {
            let query = std::fs::read_to_string(query_file)?;
            let metadata = api.register_query(id.clone(), &query)?;
            println!("✓ Registered query: {}", id);
            println!("  RSP-QL: {}", metadata.parsed.rspql_query);
            println!("  SPARQL queries: {}", metadata.parsed.sparql_queries.len());
        }
        
        Commands::Execute { id, limit } => {
            println!("Starting query: {}", id);
            let handle = api.start_query(&id)?;
            
            println!("Receiving results (limit: {})...\n", limit);
            
            for i in 0..limit {
                if let Some(result) = handle.receive() {
                    println!("Result {} [{}]:", i + 1, 
                        if result.source == ResultSource::Historical {
                            "Historical"
                        } else {
                            "Live"
                        }
                    );
                    
                    for binding in result.bindings {
                        println!("  {:?}", binding);
                    }
                } else {
                    break;
                }
            }
        }
    }
    
    Ok(())
}
```

**Add to `Cargo.toml`:**
```toml
[[bin]]
name = "query_cli"
path = "src/bin/query_cli.rs"
```

**Test it:**
```bash
cargo build --bin query_cli
./target/debug/query_cli --help
```

---

### Task 6: Write Integration Test (2 hours) 🟡

**File:** `tests/mvp_integration_test.rs`

```rust
use janus::{
    api::janus_api::{JanusApi, ResultSource},
    core::RDFEvent,
    parsing::janusql_parser::JanusQLParser,
    registry::query_registry::QueryRegistry,
    storage::{segmented_storage::StreamingSegmentedStorage, util::StreamingConfig},
};
use std::sync::Arc;
use tempfile::tempdir;

#[test]
fn test_mvp_hybrid_query_execution() {
    // Setup
    let temp_dir = tempdir().unwrap();
    let storage_path = temp_dir.path().join("storage");
    
    let storage = Arc::new(
        StreamingSegmentedStorage::new(&storage_path, StreamingConfig::default())
            .unwrap()
    );
    
    let parser = JanusQLParser::new();
    let registry = Arc::new(QueryRegistry::new());
    let api = JanusApi::new(parser, registry, Arc::clone(&storage)).unwrap();
    
    // Ingest historical data
    let events = vec![
        RDFEvent::new(
            1000,
            "http://example.org/sensor1",
            "http://example.org/temperature",
            "23.5",
            "http://example.org/graph1"
        ),
    ];
    
    storage.write(&events).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(200));
    
    // Register query
    let query = r#"
        PREFIX ex: <http://example.org/>
        REGISTER RStream <output> AS
        SELECT ?s ?temp
        FROM NAMED WINDOW ex:w1 ON STREAM ex:stream1 [RANGE 60000 STEP 10000]
        WHERE {
            WINDOW ex:w1 { ?s ex:temperature ?temp }
        }
    "#;
    
    api.register_query("test_query".to_string(), query).unwrap();
    
    // Start query
    let handle = api.start_query(&"test_query".to_string()).unwrap();
    
    // Receive results
    let mut historical_count = 0;
    for _ in 0..10 {
        if let Some(result) = handle.try_receive() {
            if result.source == ResultSource::Historical {
                historical_count += 1;
            }
        } else {
            break;
        }
    }
    
    assert!(historical_count > 0, "Should receive historical results");
}
```

**Run it:**
```bash
cargo test --test mvp_integration_test
```

---

## Testing Your MVP

### Step 1: Prepare Test Data

```bash
# Create test data file
cat > data/test_sensors.nq << 'EOF'
<http://example.org/sensor1> <http://example.org/temperature> "23.5" <http://example.org/graph1> .
<http://example.org/sensor2> <http://example.org/temperature> "24.1" <http://example.org/graph1> .
<http://example.org/sensor3> <http://example.org/temperature> "22.8" <http://example.org/graph1> .
EOF
```

### Step 2: Ingest Historical Data

```bash
cargo run --bin stream_bus_cli -- \
  --input data/test_sensors.nq \
  --broker none \
  --add-timestamps \
  --storage-path ./data/janus_storage
```

### Step 3: Create Query File

```bash
cat > data/test_query.janusql << 'EOF'
PREFIX ex: <http://example.org/>
REGISTER RStream <output> AS
SELECT ?sensor ?temp
FROM NAMED WINDOW ex:historical ON STREAM ex:stream1 [RANGE 3600000 STEP 600000]
WHERE {
    WINDOW ex:historical { ?sensor ex:temperature ?temp }
}
EOF
```

### Step 4: Register Query

```bash
cargo run --bin query_cli -- \
  --storage ./data/janus_storage \
  register \
  --id temp_monitor \
  --query-file data/test_query.janusql
```

### Step 5: Execute Query

```bash
cargo run --bin query_cli -- \
  --storage ./data/janus_storage \
  execute \
  --id temp_monitor \
  --limit 10
```

**Expected output:**
```
Starting query: temp_monitor
Receiving results (limit: 10)...

Result 1 [Historical]:
  {"?sensor": "http://example.org/sensor1", "?temp": "23.5"}
Result 2 [Historical]:
  {"?sensor": "http://example.org/sensor2", "?temp": "24.1"}
Result 3 [Historical]:
  {"?sensor": "http://example.org/sensor3", "?temp": "22.8"}
```

---

## Troubleshooting

### "Query not found"
- Make sure you registered the query first
- Check storage path is consistent

### No historical results
- Verify data was ingested: `ls -lh data/janus_storage/`
- Check time ranges in query match ingested data timestamps
- Add debug logging to `HistoricalExecutor`

### No live results
- EventBus integration not complete yet (Phase 2)
- For MVP, focus on historical path first

### SPARQL errors
- Check query syntax in generated SPARQL
- Print `metadata.parsed.sparql_queries` in CLI

---

## Success Criteria Checklist

- [ ] Task 1: SPARQL bindings format fixed
- [ ] Task 2: HistoricalExecutor implemented
- [ ] Task 3: EventBus created
- [ ] Task 4: `start_query()` working
- [ ] Task 5: Query CLI functional
- [ ] Task 6: Integration test passing
- [ ] Can register query via CLI
- [ ] Can execute query via CLI
- [ ] Receive historical results
- [ ] Results formatted correctly
- [ ] No panics or crashes

---

## After MVP Works

Once you have historical queries working:

1. **Add EventBus to live worker** in `start_query()`
2. **Test live processing** by streaming new data
3. **Add HTTP/WebSocket API** for Flutter dashboard
4. **Docker Compose** for Kafka/MQTT testing
5. **Production hardening** (logging, monitoring, error handling)

---

## Questions?

Refer to:
- **`MVP_TODO.md`** - Detailed task breakdown
- **`MVP_ARCHITECTURE.md`** - Architecture diagrams
- **`STREAM_BUS_CLI.md`** - Data ingestion docs
- **`.github/copilot-instructions.md`** - Code conventions

**Key insight:** You're 80% there! The storage, parser, and engines all work. You just need the coordinator (`start_query()`) to orchestrate them. Start with historical path (easier), then add live.