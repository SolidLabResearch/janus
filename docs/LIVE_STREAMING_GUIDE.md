# Janus Live Streaming Guide

## Overview

Janus now supports **hybrid queries** that combine historical data retrieval with live stream processing via MQTT. This guide explains how the live streaming integration works and how to use it.

## Architecture

### Components

1. **StreamBus (Publisher)**
   - Reads RDF data from files
   - Publishes events to MQTT broker
   - Writes events to storage

2. **MqttSubscriber (Subscriber)**
   - Subscribes to MQTT topics
   - Receives RDF events
   - Feeds events to LiveStreamProcessing

3. **LiveStreamProcessing (Query Engine)**
   - Processes RSP-QL queries on live streams
   - Maintains sliding windows
   - Produces query results

4. **JanusApi (Coordinator)**
   - Orchestrates historical + live execution
   - Spawns MQTT subscribers when queries start
   - Merges results from both sources

### Data Flow

```
File → StreamBus → MQTT Broker → MqttSubscriber → LiveStreamProcessing → Results
                 ↓
              Storage → HistoricalExecutor → Results
```

## Quick Start

### 1. Start MQTT Broker

```bash
docker-compose up -d mosquitto
```

### 2. Start HTTP Server

```bash
./start_http_server.sh --clean
```

### 3. Open Dashboard

Open `examples/demo_dashboard.html` in your browser.

### 4. Start Replay (Publishes to MQTT + Storage)

Click "Start Replay" in the dashboard. This:
- Reads data from `data/sensors_correct.nq`
- Publishes to MQTT topic "sensors"
- Writes to storage at `data/storage/`
- Loops the file continuously

### 5. Wait for Storage Flush

Wait 10 seconds for historical data to flush to disk.

### 6. Start Query

Click "Start Query". This:
- Registers a hybrid query with both historical and live windows
- Spawns MQTT subscriber to receive live events
- Executes historical query on stored data
- Streams both results to WebSocket

## Query Structure

### Hybrid Query Example

```sparql
PREFIX ex: <http://example.org/>
REGISTER RStream ex:output AS
SELECT ?sensor ?temp
FROM NAMED WINDOW ex:histWindow ON STREAM ex:sensorStream [START 1000000000000 END 2000000000000]
FROM NAMED WINDOW ex:liveWindow ON STREAM ex:sensorStream [RANGE 5000 STEP 2000]
WHERE {
  WINDOW ex:histWindow {
    ?sensor ex:temperature ?temp .
  }
  WINDOW ex:liveWindow {
    ?sensor ex:temperature ?temp .
  }
}
```

### Window Types

**Historical Window (START/END)**
- Queries past data from storage
- Fixed time range: `[START timestamp END timestamp]`
- Example: `[START 1000000000000 END 2000000000000]`

**Live Window (RANGE/STEP)**
- Queries streaming data from MQTT
- Sliding window: `[RANGE duration STEP slide]`
- Example: `[RANGE 5000 STEP 2000]` (5 second window, 2 second slide)

## MQTT Configuration

### Default Settings

```json
{
  "host": "localhost",
  "port": 1883,
  "client_id": "janus_live_<query_id>_<stream_name>",
  "keep_alive_secs": 30,
  "topic": "sensors"
}
```

### Topic Mapping

Currently, the MQTT topic is hardcoded to "sensors". To customize:

**In `janus_api.rs` (line ~314):**
```rust
let config = MqttSubscriberConfig {
    // ...
    topic: "your_topic_name".to_string(),
    // ...
};
```

Future improvement: Map stream URIs to MQTT topics via configuration.

## Data Format

### N-Triples (3 components - no graph)

```ntriples
<http://example.org/sensor1> <http://example.org/temperature> "23.5" .
```

This is the **recommended format** for default graph queries.

### N-Quads (4 components - with graph)

```nquads
<http://example.org/sensor1> <http://example.org/temperature> "23.5" <http://example.org/graph1> .
```

If using named graphs, add `GRAPH` clauses to your SPARQL WHERE clause:

```sparql
WHERE {
  WINDOW ex:histWindow {
    GRAPH ex:graph1 {
      ?sensor ex:temperature ?temp .
    }
  }
}
```

## Result Format

Results stream via WebSocket as JSON:

```json
{
  "query_id": "demo_query",
  "timestamp": 1736929200000,
  "source": "live",
  "bindings": [
    {
      "sensor": "http://example.org/sensor1",
      "temp": "\"23.5\""
    }
  ]
}
```

### Result Sources

- `"historical"` - From storage query (appears once per historical window)
- `"live"` - From MQTT stream (appears continuously as events arrive)

## Troubleshooting

### Empty Sensor Values in Results

**Symptom:** Bindings show `"sensor": ""`

**Causes:**
1. Data has named graphs but query doesn't specify `GRAPH` clause
2. Old storage data with different format
3. Dictionary encoding issue

**Fix:**
```bash
# Clean storage and restart
rm -rf data/storage/*
./start_http_server.sh
```

### No Live Results

**Symptom:** Only historical results appear, no live results

**Causes:**
1. MQTT broker not running
2. Query started before replay (subscriber had nothing to subscribe to)
3. MQTT topic mismatch

**Fix:**
```bash
# Verify MQTT broker
docker ps | grep mosquitto

# Monitor MQTT messages
docker exec -it janus-mosquitto mosquitto_sub -t "sensors" -v

# Check server logs
tail -f /tmp/janus_server.log
```

### No Historical Results

**Symptom:** Only live results appear, no historical results

**Causes:**
1. Storage not flushed yet (wait longer)
2. Timestamp mismatch (query window doesn't match data timestamps)
3. Storage directory empty

**Fix:**
```bash
# Check storage contents
ls -lh data/storage/

# Verify timestamp window
# Historical window should be: [START 1000000000000 END 2000000000000]
# This covers ~2001-2033 (current timestamps when add_timestamps: true)
```

### MQTT Publish Errors

**Symptom:** Server logs show "MQTT publish error"

**Causes:**
1. Mosquitto not ready when replay starts
2. Network issues
3. Invalid MQTT configuration

**Fix:**
```bash
# Restart mosquitto
docker-compose restart mosquitto

# For historical-only testing, use broker_type: "none"
```

## Testing Script

Use the automated test script:

```bash
./test_live_streaming.sh
```

This script:
1. Verifies MQTT broker is running
2. Cleans storage
3. Builds and starts the server
4. Starts replay with MQTT
5. Registers and executes a hybrid query
6. Monitors results for 15 seconds
7. Cleans up

## Implementation Details

### MQTT Subscriber Lifecycle

**When query starts:**
```rust
// 1. Create shared LiveStreamProcessing
let live_processor = Arc::new(Mutex::new(LiveStreamProcessing::new(rspql)?));

// 2. Register streams
processor.register_stream(&stream_uri);
processor.start_processing();

// 3. Spawn MQTT subscriber
let subscriber = Arc::new(MqttSubscriber::new(config));
thread::spawn(move || {
    subscriber.start(live_processor); // Blocks and feeds events
});
```

**When query stops:**
```rust
// 1. Send shutdown signal to live worker
shutdown_tx.send(());

// 2. Stop MQTT subscriber
subscriber.stop(); // Sets atomic flag to exit event loop
```

### Thread Model

- **Main thread:** HTTP server (Axum)
- **Replay thread:** StreamBus publishing to MQTT
- **MQTT subscriber thread:** Receiving events, feeding to LiveStreamProcessing
- **Live worker thread:** Polling LiveStreamProcessing for results
- **Historical worker threads:** One per historical window

### Synchronization

- `Arc<Mutex<LiveStreamProcessing>>` shared between MQTT subscriber and live worker
- Brief lock acquisitions: subscriber to `add_event()`, worker to `try_receive_result()`
- Lock released between polls to prevent blocking

## Performance Considerations

### MQTT Throughput

- Default QoS: AtLeastOnce
- Connection pool: 100 messages
- Current implementation: Single-threaded per query

**For high throughput:**
- Consider using QoS 0 (AtMostOnce) for lower latency
- Increase connection pool size in `AsyncClient::new(mqttoptions, 100)`
- Use multiple MQTT subscribers for different topics/streams

### Memory Usage

- LiveStreamProcessing maintains in-memory windows
- Window size controlled by RANGE parameter
- Old events automatically evicted by window logic

### Latency

- End-to-end latency: ~10-50ms (MQTT → subscriber → processor → result)
- Worker polling interval: 10ms
- Can reduce for lower latency (increases CPU usage)

## Future Improvements

1. **Topic Mapping:** Configure MQTT topic per stream URI
2. **Multiple Brokers:** Support subscribing to different MQTT brokers per stream
3. **Kafka Support:** Add Kafka subscriber alongside MQTT
4. **Backpressure:** Handle slow consumers gracefully
5. **Metrics:** Expose MQTT subscriber metrics in `/api/replay/status`
6. **Reconnection:** Better retry logic for MQTT connection failures
7. **Dynamic Registration:** Add/remove streams without stopping query

## API Reference

### Start Replay with MQTT

```bash
POST /api/replay/start
Content-Type: application/json

{
  "input_file": "data/sensors_correct.nq",
  "broker_type": "mqtt",
  "topics": ["sensors"],
  "rate_of_publishing": 500,
  "loop_file": true,
  "add_timestamps": true,
  "mqtt_config": {
    "host": "localhost",
    "port": 1883,
    "client_id": "janus_replay",
    "keep_alive_secs": 30
  }
}
```

### Register Hybrid Query

```bash
POST /api/queries
Content-Type: application/json

{
  "query_id": "my_query",
  "janusql": "PREFIX ex: <http://example.org/>..."
}
```

### Start Query (Auto-spawns MQTT Subscribers)

```bash
POST /api/queries/my_query/start
```

### Stream Results via WebSocket

```javascript
const ws = new WebSocket('ws://localhost:8080/api/queries/my_query/results');
ws.onmessage = (event) => {
  const result = JSON.parse(event.data);
  console.log(result.source, result.bindings);
};
```

### Stop Query (Auto-stops MQTT Subscribers)

```bash
DELETE /api/queries/my_query
```

## Summary

Janus now provides complete live streaming support via MQTT integration:

✓ StreamBus publishes to MQTT  
✓ MqttSubscriber feeds events to LiveStreamProcessing  
✓ Hybrid queries combine historical + live data  
✓ WebSocket streams results in real-time  
✓ Auto-cleanup when queries stop  

For questions or issues, check the server logs at `/tmp/janus_server.log`.