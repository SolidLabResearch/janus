# Janus MVP Architecture Overview

## Current State vs. Target State

### Legend
- ✅ **Implemented & Working**
- ⚠️ **Partially Implemented**
- ❌ **Missing / Not Implemented**

---

## System Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          JANUS HYBRID RDF ENGINE                             │
└─────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────┐
│                              CLIENT LAYER                                    │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                               │
│  ✅ Stream Bus CLI          ❌ Query CLI           ❌ HTTP/WebSocket API    │
│  (Data Ingestion)           (Query Execution)      (Dashboard Integration)   │
│                                                                               │
│  $ stream_bus_cli           $ query_cli            REST + WebSocket          │
│    --input data.nq            --register q1        GET /api/queries          │
│    --storage path             --execute q1         POST /api/queries/:id     │
│    --rate 1000                --format json        WS /api/queries/:id/results│
│                                                                               │
└───────────────────────┬───────────────────┬─────────────────────────────────┘
                        │                   │
                        │                   │
┌───────────────────────▼───────────────────▼─────────────────────────────────┐
│                           JANUS API LAYER                                    │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                               │
│  ⚠️  JanusApi (src/api/janus_api.rs)                                        │
│                                                                               │
│  ✅ register_query(query_id, janusql) → QueryMetadata                       │
│      ├─ Parses JanusQL via JanusQLParser                                    │
│      ├─ Stores in QueryRegistry                                             │
│      └─ Returns metadata                                                     │
│                                                                               │
│  ❌ start_query(query_id) → QueryHandle  <-- CRITICAL MISSING PIECE        │
│      ├─ ❌ Spawn Historical Worker                                          │
│      │   ├─ Query storage for time range                                    │
│      │   ├─ Decode Event → RDFEvent                                         │
│      │   ├─ Execute SPARQL via OxigraphAdapter                              │
│      │   └─ Send results with ResultSource::Historical                      │
│      │                                                                        │
│      ├─ ❌ Spawn Live Worker                                                │
│      │   ├─ Initialize LiveStreamProcessing                                 │
│      │   ├─ Subscribe to EventBus for incoming events                       │
│      │   ├─ Add events to RSP engine                                        │
│      │   └─ Send results with ResultSource::Live                            │
│      │                                                                        │
│      └─ Return QueryHandle { query_id, receiver }                           │
│                                                                               │
│  ❌ stop_query(query_id) → Result<(), Error>                                │
│      └─ Send shutdown signals, join threads                                 │
│                                                                               │
└─────────────────────────────────────────────────────────────────────────────┘
         │                              │                          │
         │                              │                          │
         ▼                              ▼                          ▼
┌────────────────┐          ┌──────────────────┐      ┌─────────────────────┐
│ ✅ QueryRegistry│          │ ✅ JanusQLParser │      │ ❌ Event Bus        │
├────────────────┤          ├──────────────────┤      ├─────────────────────┤
│ Stores queries │          │ Parses JanusQL   │      │ Pub/Sub for events  │
│ with metadata  │          │ Generates:       │      │                     │
│                │          │ - RSP-QL         │      │ publish(event)      │
│ register()     │          │ - SPARQL         │      │ subscribe() → rx    │
│ get()          │          │ - Windows        │      │                     │
│ unregister()   │          │ - Prefixes       │      │ Connects:           │
│ list_all()     │          │                  │      │ StreamBus → Live    │
└────────────────┘          └──────────────────┘      └─────────────────────┘


┌─────────────────────────────────────────────────────────────────────────────┐
│                        DATA INGESTION LAYER                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                               │
│  ✅ StreamBus (src/stream_bus/stream_bus.rs)                                │
│                                                                               │
│  Input: RDF file (N-Triples/N-Quads)                                        │
│    │                                                                          │
│    ├─► Parse RDF lines → RDFEvent                                           │
│    │                                                                          │
│    ├─► Write to Storage (via Dictionary encoding)                           │
│    │   └─ Event (24 bytes) = u32 IDs + u64 timestamp                        │
│    │                                                                          │
│    ├─► ❌ Publish to EventBus (for live processing)  <-- MISSING            │
│    │                                                                          │
│    └─► Publish to Kafka/MQTT (optional)                                     │
│                                                                               │
└─────────────────────────────────────────────────────────────────────────────┘
                                  │
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                       STORAGE & INDEXING LAYER                               │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                               │
│  ✅ StreamingSegmentedStorage (src/storage/segmented_storage.rs)            │
│                                                                               │
│  Architecture:                                                               │
│                                                                               │
│  ┌──────────────────┐       Background Thread                               │
│  │  BatchBuffer     │◄──────────────────────────────┐                       │
│  │  (Arc<RwLock>)   │                                │                       │
│  └────────┬─────────┘                                │                       │
│           │                                          │                       │
│           │ Flush when threshold exceeded            │                       │
│           │                                          │                       │
│           ▼                                          │                       │
│  ┌──────────────────────────────────────────────────┴─────┐                 │
│  │  Segment Files (data/ directory)                       │                 │
│  │  ├─ segment_0000.dat  (Event records, 24 bytes each)  │                 │
│  │  ├─ segment_0001.dat                                   │                 │
│  │  └─ segment_NNNN.dat                                   │                 │
│  └────────────────────────────────────────────────────────┘                 │
│           │                                                                   │
│           ▼                                                                   │
│  ┌──────────────────────────────────────────────────────┐                   │
│  │  Indexing (src/storage/indexing/)                    │                   │
│  │  ├─ Sparse Index (every Nth record)                  │                   │
│  │  ├─ Dense Index (every record)                       │                   │
│  │  └─ Dictionary (URI ←→ u32 ID mapping)               │                   │
│  └──────────────────────────────────────────────────────┘                   │
│                                                                               │
│  Key Methods:                                                                │
│  ✅ write(events: &[RDFEvent]) → Result<()>                                 │
│  ✅ read_range(start_ts, end_ts) → Result<Vec<Event>>                       │
│  ✅ background_flush_loop()                                                  │
│                                                                               │
│  Performance:                                                                │
│  - 2.6-3.14 Million quads/sec write throughput                              │
│  - Sub-millisecond point queries                                            │
│  - 40% compression (40 bytes → 24 bytes)                                    │
│                                                                               │
└─────────────────────────────────────────────────────────────────────────────┘


┌─────────────────────────────────────────────────────────────────────────────┐
│                    QUERY EXECUTION LAYER                                     │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                               │
│  ┌───────────────────────────────────────────────────────────────┐          │
│  │  HISTORICAL PATH (Batch Processing)                            │          │
│  ├───────────────────────────────────────────────────────────────┤          │
│  │                                                                 │          │
│  │  ❌ HistoricalExecutor (src/api/historical_executor.rs)       │          │
│  │     │                                                          │          │
│  │     ├─► Query storage.read_range(start_ts, end_ts)            │          │
│  │     │   └─ Returns Vec<Event> (24-byte records)               │          │
│  │     │                                                          │          │
│  │     ├─► Decode via Dictionary: Event → RDFEvent               │          │
│  │     │   └─ Expand u32 IDs to full URI strings                 │          │
│  │     │                                                          │          │
│  │     ├─► Convert RDFEvent → Oxigraph Quad                      │          │
│  │     │                                                          │          │
│  │     ├─► Build QuadContainer                                   │          │
│  │     │                                                          │          │
│  │     ├─► ⚠️ Execute SPARQL via OxigraphAdapter                 │          │
│  │     │   └─ Returns Vec<String> (needs proper binding format)  │          │
│  │     │                                                          │          │
│  │     └─► Convert to QueryResult                                │          │
│  │         └─ { query_id, timestamp, ResultSource::Historical,   │          │
│  │              bindings: Vec<HashMap<String, String>> }          │          │
│  │                                                                 │          │
│  └───────────────────────────────────────────────────────────────┘          │
│                                                                               │
│  ┌───────────────────────────────────────────────────────────────┐          │
│  │  LIVE PATH (Stream Processing)                                 │          │
│  ├───────────────────────────────────────────────────────────────┤          │
│  │                                                                 │          │
│  │  ✅ LiveStreamProcessing (src/stream/live_stream_processing.rs)│          │
│  │     │                                                          │          │
│  │     ├─► Initialize RSPEngine with RSP-QL query                │          │
│  │     │                                                          │          │
│  │     ├─► Register streams from query windows                   │          │
│  │     │                                                          │          │
│  │     ├─► start_processing() → Receiver<BindingWithTimestamp>  │          │
│  │     │                                                          │          │
│  │     ├─► ❌ Subscribe to EventBus for incoming events          │          │
│  │     │                                                          │          │
│  │     ├─► add_event(stream_uri, RDFEvent)                       │          │
│  │     │   └─ Converts to Quad, adds to RDFStream                │          │
│  │     │                                                          │          │
│  │     ├─► Windows trigger automatically (time-based)             │          │
│  │     │                                                          │          │
│  │     ├─► receive_result() / collect_results()                  │          │
│  │     │   └─ Gets BindingWithTimestamp from RSP engine           │          │
│  │     │                                                          │          │
│  │     └─► Convert to QueryResult                                │          │
│  │         └─ { query_id, timestamp, ResultSource::Live,         │          │
│  │              bindings: Vec<HashMap<String, String>> }          │          │
│  │                                                                 │          │
│  └───────────────────────────────────────────────────────────────┘          │
│                                                                               │
└─────────────────────────────────────────────────────────────────────────────┘
                                  │
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                         SPARQL ENGINES                                       │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                               │
│  ⚠️ OxigraphAdapter (src/querying/oxigraph_adapter.rs)                      │
│                                                                               │
│  execute_query(sparql: &str, container: &QuadContainer)                     │
│    → Result<Vec<String>, Error>  ⚠️ Returns debug format                    │
│                                                                               │
│  ❌ execute_query_bindings(sparql: &str, container: &QuadContainer)         │
│    → Result<Vec<HashMap<String, String>>, Error>  <-- NEEDED                │
│                                                                               │
│  ⚠️ KolibrieAdapter (stubbed, not functional)                               │
│                                                                               │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Data Flow: End-to-End Query Execution

### Scenario: Temperature Sensor Monitoring

**JanusQL Query:**
```sparql
PREFIX ex: <http://example.org/>
REGISTER RStream <output> AS
SELECT ?sensor ?temp
FROM NAMED WINDOW ex:historical ON STREAM ex:sensors [RANGE 3600000 STEP 600000]
FROM NAMED WINDOW ex:live ON STREAM ex:sensors [RANGE 5000 STEP 1000]
WHERE {
    WINDOW ?w { ?sensor ex:temperature ?temp }
}
```

### Phase 1: Registration (✅ Working)

```
User
  │
  │ query_cli --register temp_monitor --query sensors.janusql
  │
  ▼
JanusApi::register_query()
  │
  ├─► JanusQLParser::parse()
  │   ├─ Extracts windows
  │   ├─ Generates RSP-QL for live
  │   ├─ Generates SPARQL for historical
  │   └─ Returns ParsedJanusQuery
  │
  └─► QueryRegistry::register()
      └─ Stores metadata with query_id
```

### Phase 2: Historical Data Ingestion (✅ Working)

```
Historical Data File: sensors_historical.nq
  │
  │ <http://ex.org/s1> <http://ex.org/temp> "23.5" <http://ex.org/g1> .
  │ <http://ex.org/s2> <http://ex.org/temp> "24.1" <http://ex.org/g1> .
  │
  │ stream_bus_cli --input sensors_historical.nq --broker none --storage-path ./data
  │
  ▼
StreamBus::run()
  │
  ├─► parse_rdf_line() → RDFEvent
  │   └─ RDFEvent { timestamp: 1000, subject: "http://...", ... }
  │
  └─► StreamingSegmentedStorage::write()
      │
      ├─► Dictionary::encode() → Event
      │   ├─ "http://ex.org/s1" → ID: 1
      │   ├─ "http://ex.org/temp" → ID: 2
      │   ├─ "23.5" → ID: 3
      │   └─ Event { s: 1, p: 2, o: 3, g: 0, ts: 1000 }  (24 bytes)
      │
      └─► BatchBuffer::push()
          └─ Background thread flushes to segment files
```

### Phase 3: Query Execution Start (❌ Not Implemented)

```
User
  │
  │ query_cli --execute temp_monitor --format json
  │
  ▼
JanusApi::start_query("temp_monitor")
  │
  ├─► Validate query exists
  │
  ├─► Create result channel
  │   └─ (result_tx, result_rx) = mpsc::channel()
  │
  ├─► ❌ Spawn HISTORICAL WORKER Thread
  │   │
  │   ├─► Parse historical windows
  │   │   └─ Window: RANGE 3600000 STEP 600000
  │   │       → Query last hour in 10-minute chunks
  │   │
  │   ├─► For each time window [start_ts, end_ts]:
  │   │   │
  │   │   ├─► storage.read_range(start_ts, end_ts)
  │   │   │   └─ Returns Vec<Event> (encoded)
  │   │   │
  │   │   ├─► Dictionary::decode() each Event → RDFEvent
  │   │   │   └─ ID: 1 → "http://ex.org/s1"
  │   │   │
  │   │   ├─► Convert RDFEvent → Oxigraph Quad
  │   │   │   └─ Quad { s: NamedNode, p: NamedNode, o: Literal, g: ... }
  │   │   │
  │   │   ├─► Build QuadContainer(quads, end_ts)
  │   │   │
  │   │   ├─► OxigraphAdapter::execute_query_bindings(sparql, container)
  │   │   │   └─ Returns Vec<HashMap<"?sensor", "http://...">, ...>
  │   │   │
  │   │   └─► Send QueryResult
  │   │       └─ result_tx.send(QueryResult {
  │   │             query_id: "temp_monitor",
  │   │             timestamp: end_ts,
  │   │             source: ResultSource::Historical,
  │   │             bindings: [{
  │   │                 "?sensor": "http://ex.org/s1",
  │   │                 "?temp": "23.5"
  │   │             }]
  │   │          })
  │   │
  │   └─► Complete (historical data exhausted)
  │
  ├─► ❌ Spawn LIVE WORKER Thread
  │   │
  │   ├─► LiveStreamProcessing::new(rspql_query)
  │   │
  │   ├─► register_stream("http://ex.org/sensors")
  │   │
  │   ├─► start_processing()
  │   │
  │   ├─► ❌ Subscribe to EventBus
  │   │   └─ event_rx = event_bus.subscribe()
  │   │
  │   └─► Loop:
  │       │
  │       ├─► event_rx.try_recv() → RDFEvent
  │       │
  │       ├─► LiveStreamProcessing::add_event(stream_uri, event)
  │       │   ├─ Converts to Quad
  │       │   ├─ Adds to RDFStream
  │       │   └─ RSP engine processes windows
  │       │
  │       ├─► try_receive_result() → BindingWithTimestamp
  │       │
  │       └─► Send QueryResult
  │           └─ result_tx.send(QueryResult {
  │                 query_id: "temp_monitor",
  │                 timestamp: result.timestamp,
  │                 source: ResultSource::Live,
  │                 bindings: convert_bindings(result)
  │              })
  │
  └─► Return QueryHandle { query_id, receiver: result_rx }
```

### Phase 4: Live Data Ingestion (❌ EventBus Integration Missing)

```
Live Data Stream
  │
  │ <http://ex.org/s3> <http://ex.org/temp> "25.0" .
  │
  │ stream_bus_cli --input - --broker none --add-timestamps
  │
  ▼
StreamBus::run()
  │
  ├─► parse_rdf_line() → RDFEvent
  │
  ├─► storage.write(&[event])  ✅ Works
  │
  └─► ❌ event_bus.publish(event)  <-- MISSING
      │
      └─► EventBus distributes to subscribers
          │
          └─► Live Worker receives event
              └─► Adds to LiveStreamProcessing
```

### Phase 5: Result Consumption (✅ QueryHandle API exists)

```
QueryHandle
  │
  ├─► handle.receive() → blocks for next result
  │   │
  │   └─► QueryResult {
  │         query_id: "temp_monitor",
  │         timestamp: 1640000000,
  │         source: Historical | Live,
  │         bindings: [{ "?sensor": "...", "?temp": "23.5" }]
  │       }
  │
  └─► User displays results (CLI table, JSON, or WebSocket to Flutter)
```

---

## Critical Missing Components Summary

### 1. JanusApi::start_query() Implementation
- **Status:** ❌ Commented out (lines 128-140 in janus_api.rs)
- **Impact:** Cannot execute queries at all
- **Effort:** High (200-300 lines, complex threading)
- **Priority:** 🔴 CRITICAL

### 2. HistoricalExecutor
- **Status:** ❌ Doesn't exist
- **Impact:** No historical query results
- **Effort:** Medium (150-200 lines)
- **Priority:** 🔴 CRITICAL

### 3. EventBus for Live Integration
- **Status:** ❌ Doesn't exist
- **Impact:** No live query results
- **Effort:** Medium (100-150 lines)
- **Priority:** 🔴 CRITICAL

### 4. SPARQL Result Formatting
- **Status:** ⚠️ Returns debug strings, not structured bindings
- **Impact:** Results are unparseable
- **Effort:** Low (50-75 lines)
- **Priority:** 🔴 CRITICAL

### 5. Query Execution CLI
- **Status:** ❌ Doesn't exist (only ingestion CLI exists)
- **Impact:** No user interface for queries
- **Effort:** Medium (200-250 lines)
- **Priority:** 🟠 HIGH

### 6. End-to-End Integration Test
- **Status:** ❌ Doesn't exist
- **Impact:** Can't validate MVP works
- **Effort:** Medium (150-200 lines)
- **Priority:** 🟠 HIGH

---

## Thread Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         Main Thread                              │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  - Accept API calls (register_query, start_query, stop_query)   │
│  - Manage running queries map                                    │
│  - Return QueryHandle to caller                                  │
│                                                                   │
└───────────┬──────────────────────────────┬──────────────────────┘
            │                              │
            │ Spawns                       │ Spawns
            │                              │
            ▼                              ▼
┌─────────────────────────┐    ┌─────────────────────────────────┐
│  Historical Worker      │    │  Live Worker Thread             │
│  Thread                 │    │                                 │
├─────────────────────────┤    ├─────────────────────────────────┤
│                         │    │                                 │
│ Loop over time windows  │    │ Loop:                           │
│   ├─ Query storage      │    │   ├─ Receive events from bus   │
│   ├─ Decode events      │    │   ├─ Add to LiveProcessing     │
│   ├─ Execute SPARQL     │    │   ├─ Poll for results          │
│   └─ Send results       │    │   └─ Send results              │
│                         │    │                                 │
│ Listens for shutdown    │    │ Listens for shutdown            │
│                         │    │                                 │
└─────────────────────────┘    └─────────────────────────────────┘
            │                              │
            │ Sends via mpsc::Sender       │ Sends via mpsc::Sender
            │                              │
            ▼                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                   Result Channel (mpsc)                          │
│                                                                   │
│  QueryHandle holds mpsc::Receiver                                │
│  ├─ receive() blocks for next result                             │
│  └─ try_receive() non-blocking                                   │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

---

## Data Model Reference

### RDFEvent (User-facing)
```rust
pub struct RDFEvent {
    pub timestamp: u64,
    pub subject: String,      // Full URI: "http://example.org/alice"
    pub predicate: String,    // Full URI: "http://example.org/knows"
    pub object: String,       // Full URI or literal: "Bob" or "http://..."
    pub graph: String,        // Full URI: "http://example.org/graph1"
}
```

### Event (Storage-internal, 24 bytes)
```rust
pub struct Event {
    pub subject: u32,      // Dictionary ID
    pub predicate: u32,    // Dictionary ID
    pub object: u32,       // Dictionary ID
    pub graph: u32,        // Dictionary ID
    pub timestamp: u64,    // Milliseconds since epoch
}
```

### QueryResult (Output)
```rust
pub struct QueryResult {
    pub query_id: QueryId,
    pub timestamp: u64,
    pub source: ResultSource,  // Historical | Live
    pub bindings: Vec<HashMap<String, String>>,
}

// Example:
QueryResult {
    query_id: "temp_monitor",
    timestamp: 1640000000,
    source: ResultSource::Historical,
    bindings: vec![
        HashMap::from([
            ("?sensor".to_string(), "http://example.org/sensor1".to_string()),
            ("?temp".to_string(), "23.5".to_string()),
        ]),
    ],
}
```

---

## Next Steps

See **`MVP_TODO.md`** for detailed implementation tasks, estimates, and priority order.

**Quick Start:**
1. Implement `OxigraphAdapter::execute_query_bindings()` (easiest)
2. Create `HistoricalExecutor` (foundational)
3. Create `EventBus` (enables live)
4. Implement `JanusApi::start_query()` (ties it all together)
5. Write integration test (validates MVP)
6. Build Query CLI (makes it usable)