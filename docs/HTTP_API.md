# Janus HTTP API Documentation

## Overview

The Janus HTTP API provides REST endpoints for query management and WebSocket streaming for real-time results. It also includes stream bus replay control endpoints for demo and testing purposes.

**Base URL:** `http://localhost:8080`

## Quick Start

### 1. Start the HTTP Server

```bash
# Build and run the HTTP server
cargo run --bin http_server

# With custom configuration
cargo run --bin http_server -- --host 0.0.0.0 --port 8080 --storage-dir ./data/storage
```

### 2. Run the Example Client

```bash
# Run the comprehensive client example
cargo run --example http_client_example
```

## Architecture

The HTTP API server provides:

- **REST Endpoints**: JSON-based HTTP endpoints for query registration, lifecycle management, and replay control
- **WebSocket Streaming**: Real-time streaming of query results (both historical and live)
- **CORS Support**: Cross-Origin Resource Sharing enabled for dashboard integration
- **Thread-Safe State**: Shared state using `Arc` for concurrent access across async tasks

## API Endpoints

### Health Check

#### `GET /health`

Health check endpoint to verify server is running.

**Response:**
```json
{
  "message": "Janus HTTP API is running"
}
```

---

### Query Management

#### `POST /api/queries`

Register a new JanusQL query.

**Request Body:**
```json
{
  "query_id": "sensor_query_1",
  "janusql": "SELECT ?sensor ?temp FROM HISTORICAL FIXED WINDOW [2024-01-01T00:00:00Z, 2024-01-02T00:00:00Z] WHERE { ?sensor <http://example.org/temperature> ?temp . }"
}
```

**Response (201 Created):**
```json
{
  "query_id": "sensor_query_1",
  "query_text": "SELECT ?sensor ?temp FROM...",
  "registered_at": 1704067200,
  "message": "Query registered successfully"
}
```

**Error Response (400 Bad Request):**
```json
{
  "error": "Parse Error: Failed to parse JanusQL query: ..."
}
```

---

#### `GET /api/queries`

List all registered queries.

**Response:**
```json
{
  "queries": [
    "sensor_query_1",
    "live_sensor_query",
    "historical_analysis"
  ],
  "total": 3
}
```

---

#### `GET /api/queries/:id`

Get details for a specific query.

**Parameters:**
- `id` (path): Query identifier

**Response:**
```json
{
  "query_id": "sensor_query_1",
  "query_text": "SELECT ?sensor ?temp FROM...",
  "registered_at": 1704067200,
  "execution_count": 5,
  "is_running": true,
  "status": "Running"
}
```

**Status Values:**
- `Registered` - Query registered but not started
- `Running` - Query is currently executing
- `Stopped` - Query was stopped
- `Failed` - Query execution failed
- `Completed` - Query execution completed

**Error Response (404 Not Found):**
```json
{
  "error": "Query 'nonexistent' not found"
}
```

---

#### `POST /api/queries/:id/start`

Start executing a registered query.

**Parameters:**
- `id` (path): Query identifier

**Response:**
```json
{
  "message": "Query 'sensor_query_1' started successfully"
}
```

**Error Responses:**

Already Running (400):
```json
{
  "error": "Execution Error: Query 'sensor_query_1' is already running"
}
```

Not Found (404):
```json
{
  "error": "Query 'sensor_query_1' not found"
}
```

---

#### `DELETE /api/queries/:id`

Stop a running query.

**Parameters:**
- `id` (path): Query identifier

**Response:**
```json
{
  "message": "Query 'sensor_query_1' stopped successfully"
}
```

**Error Response (400 Bad Request):**
```json
{
  "error": "Execution Error: Query 'sensor_query_1' is not running"
}
```

---

#### `WS /api/queries/:id/results`

WebSocket endpoint for streaming query results in real-time.

**Connection URL:**
```
ws://localhost:8080/api/queries/sensor_query_1/results
```

**Message Format:**
```json
{
  "query_id": "sensor_query_1",
  "timestamp": 1704067200000,
  "source": "historical",
  "bindings": [
    {
      "sensor": "http://example.org/sensor1",
      "temp": "23.5"
    }
  ]
}
```

**Source Types:**
- `historical` - Results from historical data processing
- `live` - Results from live stream processing

**JavaScript Example:**
```javascript
const ws = new WebSocket('ws://localhost:8080/api/queries/sensor_query_1/results');

ws.onmessage = (event) => {
  const result = JSON.parse(event.data);
  console.log(`[${result.source}] Query: ${result.query_id}`);
  console.log(`Timestamp: ${result.timestamp}`);
  console.log('Bindings:', result.bindings);
};

ws.onerror = (error) => {
  console.error('WebSocket error:', error);
};

ws.onclose = () => {
  console.log('WebSocket connection closed');
};
```

---

### Stream Bus Replay Control

#### `POST /api/replay/start`

Start the stream bus replay for ingesting RDF data.

**Request Body:**
```json
{
  "input_file": "data/sensors.nq",
  "broker_type": "none",
  "topics": ["sensors"],
  "rate_of_publishing": 1000,
  "loop_file": false,
  "add_timestamps": true,
  "kafka_config": null,
  "mqtt_config": null
}
```

**Request Parameters:**
- `input_file` (required): Path to the N-Quads input file
- `broker_type` (optional, default: "none"): Broker type - "kafka", "mqtt", or "none"
- `topics` (optional, default: ["janus"]): List of topic names
- `rate_of_publishing` (optional, default: 1000): Events per second rate limit
- `loop_file` (optional, default: false): Whether to loop the file continuously
- `add_timestamps` (optional, default: true): Add timestamps to events
- `kafka_config` (optional): Kafka broker configuration
- `mqtt_config` (optional): MQTT broker configuration

**Kafka Config:**
```json
{
  "kafka_config": {
    "bootstrap_servers": "localhost:9092",
    "client_id": "janus_client",
    "message_timeout_ms": "5000"
  }
}
```

**MQTT Config:**
```json
{
  "mqtt_config": {
    "host": "localhost",
    "port": 1883,
    "client_id": "janus_client",
    "keep_alive_secs": 30
  }
}
```

**Response:**
```json
{
  "message": "Stream bus replay started with file: data/sensors.nq"
}
```

**Error Response (400 Bad Request):**
```json
{
  "error": "Replay is already running"
}
```

---

#### `POST /api/replay/stop`

Stop the currently running stream bus replay.

**Response:**
```json
{
  "message": "Stream bus replay stopped"
}
```

**Error Response (400 Bad Request):**
```json
{
  "error": "Replay is not running"
}
```

---

#### `GET /api/replay/status`

Get the current status of the stream bus replay.

**Response (Running):**
```json
{
  "is_running": true,
  "events_read": 15420,
  "events_published": 15420,
  "events_stored": 15420,
  "publish_errors": 0,
  "storage_errors": 0,
  "events_per_second": 1543.2,
  "elapsed_seconds": 10.0
}
```

**Response (Not Running):**
```json
{
  "is_running": false,
  "events_read": 0,
  "events_published": 0,
  "events_stored": 0,
  "publish_errors": 0,
  "storage_errors": 0,
  "events_per_second": 0.0,
  "elapsed_seconds": 0.0
}
```

---

## Usage Examples

### cURL Examples

#### Register a Query
```bash
curl -X POST http://localhost:8080/api/queries \
  -H "Content-Type: application/json" \
  -d '{
    "query_id": "temp_query",
    "janusql": "SELECT ?sensor ?temp FROM HISTORICAL FIXED WINDOW [2024-01-01T00:00:00Z, 2024-01-02T00:00:00Z] WHERE { ?sensor <http://example.org/temperature> ?temp . }"
  }'
```

#### List All Queries
```bash
curl http://localhost:8080/api/queries
```

#### Get Query Details
```bash
curl http://localhost:8080/api/queries/temp_query
```

#### Start a Query
```bash
curl -X POST http://localhost:8080/api/queries/temp_query/start
```

#### Stop a Query
```bash
curl -X DELETE http://localhost:8080/api/queries/temp_query
```

#### Start Replay
```bash
curl -X POST http://localhost:8080/api/replay/start \
  -H "Content-Type: application/json" \
  -d '{
    "input_file": "data/sensors.nq",
    "broker_type": "none",
    "topics": ["sensors"],
    "rate_of_publishing": 1000,
    "loop_file": false,
    "add_timestamps": true
  }'
```

#### Get Replay Status
```bash
curl http://localhost:8080/api/replay/status
```

#### Stop Replay
```bash
curl -X POST http://localhost:8080/api/replay/stop
```

---

### Python Example

```python
import requests
import json
from websocket import create_connection

BASE_URL = "http://localhost:8080"

# Register a query
response = requests.post(
    f"{BASE_URL}/api/queries",
    json={
        "query_id": "my_query",
        "janusql": "SELECT ?s ?p ?o FROM HISTORICAL FIXED WINDOW [2024-01-01T00:00:00Z, 2024-01-02T00:00:00Z] WHERE { ?s ?p ?o }"
    }
)
print(f"Register: {response.json()}")

# Start the query
response = requests.post(f"{BASE_URL}/api/queries/my_query/start")
print(f"Start: {response.json()}")

# Connect to WebSocket for results
ws = create_connection(f"ws://localhost:8080/api/queries/my_query/results")

# Receive results
for i in range(10):
    result = ws.recv()
    print(f"Result: {json.loads(result)}")

ws.close()

# Stop the query
response = requests.delete(f"{BASE_URL}/api/queries/my_query")
print(f"Stop: {response.json()}")
```

---

### JavaScript/Node.js Example

```javascript
const axios = require('axios');
const WebSocket = require('ws');

const BASE_URL = 'http://localhost:8080';

async function demo() {
  // Register a query
  const registerResponse = await axios.post(`${BASE_URL}/api/queries`, {
    query_id: 'js_query',
    janusql: 'SELECT ?s ?p ?o FROM HISTORICAL FIXED WINDOW [2024-01-01T00:00:00Z, 2024-01-02T00:00:00Z] WHERE { ?s ?p ?o }'
  });
  console.log('Registered:', registerResponse.data);

  // Start the query
  const startResponse = await axios.post(`${BASE_URL}/api/queries/js_query/start`);
  console.log('Started:', startResponse.data);

  // Connect to WebSocket
  const ws = new WebSocket(`ws://localhost:8080/api/queries/js_query/results`);

  ws.on('message', (data) => {
    const result = JSON.parse(data);
    console.log('Result:', result);
  });

  ws.on('error', (error) => {
    console.error('WebSocket error:', error);
  });

  // Wait for results...
  await new Promise(resolve => setTimeout(resolve, 10000));

  ws.close();

  // Stop the query
  const stopResponse = await axios.delete(`${BASE_URL}/api/queries/js_query`);
  console.log('Stopped:', stopResponse.data);
}

demo().catch(console.error);
```

---

## Dashboard Integration

### Two-Button Demo Interface

For a simple demo dashboard with "Start Replay" and "Start Query" buttons:

```html
<!DOCTYPE html>
<html>
<head>
  <title>Janus Demo Dashboard</title>
  <style>
    body { font-family: Arial, sans-serif; padding: 20px; }
    button { padding: 10px 20px; margin: 10px; font-size: 16px; }
    .success { color: green; }
    .error { color: red; }
    #results { margin-top: 20px; border: 1px solid #ccc; padding: 10px; max-height: 400px; overflow-y: auto; }
  </style>
</head>
<body>
  <h1>Janus RDF Stream Processing - Demo</h1>
  
  <button id="startReplay" onclick="startReplay()">Start Replay</button>
  <button id="stopReplay" onclick="stopReplay()" disabled>Stop Replay</button>
  <br>
  <button id="startQuery" onclick="startQuery()">Start Query</button>
  <button id="stopQuery" onclick="stopQuery()" disabled>Stop Query</button>
  
  <div id="status"></div>
  <div id="results"></div>

  <script>
    const API_BASE = 'http://localhost:8080';
    const QUERY_ID = 'demo_query';
    let ws = null;

    async function startReplay() {
      try {
        const response = await fetch(`${API_BASE}/api/replay/start`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            input_file: 'data/sensors.nq',
            broker_type: 'none',
            topics: ['sensors'],
            rate_of_publishing: 1000,
            loop_file: true,
            add_timestamps: true
          })
        });
        
        const data = await response.json();
        
        if (response.ok) {
          showStatus(data.message, 'success');
          document.getElementById('startReplay').disabled = true;
          document.getElementById('stopReplay').disabled = false;
          pollReplayStatus();
        } else {
          showStatus(data.error, 'error');
        }
      } catch (error) {
        showStatus(`Error: ${error.message}`, 'error');
      }
    }

    async function stopReplay() {
      try {
        const response = await fetch(`${API_BASE}/api/replay/stop`, {
          method: 'POST'
        });
        
        const data = await response.json();
        
        if (response.ok) {
          showStatus(data.message, 'success');
          document.getElementById('startReplay').disabled = false;
          document.getElementById('stopReplay').disabled = true;
        } else {
          showStatus(data.error, 'error');
        }
      } catch (error) {
        showStatus(`Error: ${error.message}`, 'error');
      }
    }

    async function startQuery() {
      try {
        // First register the query
        const registerResponse = await fetch(`${API_BASE}/api/queries`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            query_id: QUERY_ID,
            janusql: 'SELECT ?sensor ?temp FROM HISTORICAL FIXED WINDOW [2024-01-01T00:00:00Z, 2024-01-02T00:00:00Z] WHERE { ?sensor <http://example.org/temperature> ?temp . }'
          })
        });

        if (!registerResponse.ok && registerResponse.status !== 400) {
          throw new Error('Failed to register query');
        }

        // Start the query
        const startResponse = await fetch(`${API_BASE}/api/queries/${QUERY_ID}/start`, {
          method: 'POST'
        });
        
        const data = await startResponse.json();
        
        if (startResponse.ok) {
          showStatus(data.message, 'success');
          document.getElementById('startQuery').disabled = true;
          document.getElementById('stopQuery').disabled = false;
          connectWebSocket();
        } else {
          showStatus(data.error, 'error');
        }
      } catch (error) {
        showStatus(`Error: ${error.message}`, 'error');
      }
    }

    async function stopQuery() {
      try {
        const response = await fetch(`${API_BASE}/api/queries/${QUERY_ID}`, {
          method: 'DELETE'
        });
        
        const data = await response.json();
        
        if (response.ok) {
          showStatus(data.message, 'success');
          document.getElementById('startQuery').disabled = false;
          document.getElementById('stopQuery').disabled = true;
          if (ws) ws.close();
        } else {
          showStatus(data.error, 'error');
        }
      } catch (error) {
        showStatus(`Error: ${error.message}`, 'error');
      }
    }

    function connectWebSocket() {
      ws = new WebSocket(`ws://localhost:8080/api/queries/${QUERY_ID}/results`);
      
      ws.onmessage = (event) => {
        const result = JSON.parse(event.data);
        displayResult(result);
      };
      
      ws.onerror = (error) => {
        showStatus('WebSocket error', 'error');
      };
      
      ws.onclose = () => {
        showStatus('WebSocket closed', 'success');
      };
    }

    function pollReplayStatus() {
      const interval = setInterval(async () => {
        try {
          const response = await fetch(`${API_BASE}/api/replay/status`);
          const data = await response.json();
          
          if (!data.is_running) {
            clearInterval(interval);
            return;
          }
          
          document.getElementById('status').innerHTML = `
            <div class="success">
              <strong>Replay Status:</strong><br>
              Events Read: ${data.events_read}<br>
              Events Stored: ${data.events_stored}<br>
              Rate: ${data.events_per_second.toFixed(2)} events/sec<br>
              Elapsed: ${data.elapsed_seconds.toFixed(2)}s
            </div>
          `;
        } catch (error) {
          clearInterval(interval);
        }
      }, 1000);
    }

    function showStatus(message, type) {
      const statusDiv = document.getElementById('status');
      statusDiv.className = type;
      statusDiv.innerHTML = `<p><strong>${message}</strong></p>`;
    }

    function displayResult(result) {
      const resultsDiv = document.getElementById('results');
      const resultHtml = `
        <div style="border-bottom: 1px solid #eee; padding: 5px;">
          <strong>[${result.source}]</strong> 
          Timestamp: ${result.timestamp}<br>
          Bindings: ${JSON.stringify(result.bindings)}
        </div>
      `;
      resultsDiv.innerHTML = resultHtml + resultsDiv.innerHTML;
    }
  </script>
</body>
</html>
```

---

## Error Handling

All error responses follow this format:

```json
{
  "error": "Descriptive error message"
}
```

### HTTP Status Codes

- `200 OK` - Successful GET request
- `201 Created` - Successful resource creation
- `400 Bad Request` - Invalid request or operation not allowed
- `404 Not Found` - Resource not found
- `500 Internal Server Error` - Server-side error

---

## Configuration

### Server Options

```bash
Usage: http_server [OPTIONS]

Options:
  -H, --host <HOST>
          Server host address [default: 127.0.0.1]
          
  -p, --port <PORT>
          Server port [default: 8080]
          
  -s, --storage-dir <STORAGE_DIR>
          Storage directory path [default: ./data/storage]
          
      --max-batch-size-bytes <MAX_BATCH_SIZE_BYTES>
          Maximum batch size in bytes [default: 10485760]
          
      --flush-interval-ms <FLUSH_INTERVAL_MS>
          Flush interval in milliseconds [default: 5000]
          
      --max-total-memory-mb <MAX_TOTAL_MEMORY_MB>
          Maximum total memory in MB [default: 1024]
```

---

## Performance Considerations

1. **WebSocket Connections**: Each active query can have multiple WebSocket connections. Results are broadcast to all connected clients.

2. **Query Handles**: Query handles are stored in memory. Consider resource limits when running many concurrent queries.

3. **Stream Bus Replay**: Running replay at high rates (>10,000 events/sec) may impact query performance. Adjust `rate_of_publishing` accordingly.

4. **CORS**: CORS is configured to allow all origins. In production, restrict this to specific domains.

---

## Security Notes

**WARNING**: This API is designed for local development and demos. For production use:

1. Add authentication/authorization
2. Restrict CORS to specific origins
3. Add rate limiting
4. Use HTTPS/WSS instead of HTTP/WS
5. Validate and sanitize all inputs
6. Add request size limits
7. Implement proper session management

---

## Troubleshooting

### WebSocket Connection Fails

**Issue**: Cannot connect to WebSocket endpoint

**Solutions**:
- Ensure query is registered and started before connecting
- Check that the query ID in the WebSocket URL matches the registered query
- Verify the server is running and accessible
- Check browser console for CORS or connection errors

### Query Results Not Appearing

**Issue**: WebSocket connects but no results received

**Solutions**:
- Verify stream bus replay is running (`GET /api/replay/status`)
- Check query syntax is valid
- Ensure historical data exists for the specified time window
- For live queries, ensure live stream is producing events

### Replay Won't Start

**Issue**: Replay start returns error

**Solutions**:
- Check that `input_file` path exists and is accessible
- Verify no other replay is currently running
- Ensure broker configuration is correct if using Kafka/MQTT
- Check server logs for detailed error messages

---

## Additional Resources

- [JanusQL Query Language Documentation](./JANUSQL.md)
- [Stream Bus CLI Documentation](./STREAM_BUS.md)
- [Architecture Overview](./ARCHITECTURE.md)
- [Benchmark Results](./BENCHMARK_RESULTS.md)

---

## Support

For issues, feature requests, or questions:
- GitHub Issues: https://github.com/SolidLabResearch/janus/issues
- Documentation: https://github.com/SolidLabResearch/janus