# Janus HTTP API - Quick Start Guide

Get started with the Janus HTTP API in under 5 minutes.

## Prerequisites

- Rust 1.70+ installed
- Data file for testing (e.g., `data/sensors.nq`)

## 1. Start the HTTP Server

```bash
# Clone and navigate to the project
cd janus

# Build and run the HTTP server
cargo run --bin http_server

# Server will start on http://127.0.0.1:8080
```

**Custom Configuration:**
```bash
cargo run --bin http_server -- \
  --host 0.0.0.0 \
  --port 8080 \
  --storage-dir ./data/storage \
  --max-batch-size-bytes 10485760 \
  --flush-interval-ms 5000
```

## 2. Open the Demo Dashboard

Open `examples/demo_dashboard.html` in your browser:

```bash
# macOS
open examples/demo_dashboard.html

# Linux
xdg-open examples/demo_dashboard.html

# Windows
start examples/demo_dashboard.html
```

The dashboard provides two main buttons:
- **Start Replay**: Begins ingesting RDF data from file into storage
- **Start Query**: Executes a JanusQL query and streams results

## 3. Quick Test with cURL

### Register a Query
```bash
curl -X POST http://localhost:8080/api/queries \
  -H "Content-Type: application/json" \
  -d '{
    "query_id": "test_query",
    "janusql": "SELECT ?s ?p ?o FROM HISTORICAL FIXED WINDOW [2024-01-01T00:00:00Z, 2024-12-31T23:59:59Z] WHERE { ?s ?p ?o }"
  }'
```

### Start Stream Replay
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

### Start Query Execution
```bash
curl -X POST http://localhost:8080/api/queries/test_query/start
```

### Get Replay Status
```bash
curl http://localhost:8080/api/replay/status
```

### List All Queries
```bash
curl http://localhost:8080/api/queries
```

### Stop Query
```bash
curl -X DELETE http://localhost:8080/api/queries/test_query
```

## 4. WebSocket Streaming Example

### JavaScript (Browser Console)
```javascript
const ws = new WebSocket('ws://localhost:8080/api/queries/test_query/results');

ws.onmessage = (event) => {
  const result = JSON.parse(event.data);
  console.log('Query Result:', result);
  console.log('  Source:', result.source);  // 'historical' or 'live'
  console.log('  Timestamp:', result.timestamp);
  console.log('  Bindings:', result.bindings);
};

ws.onerror = (error) => console.error('WebSocket Error:', error);
ws.onclose = () => console.log('WebSocket Closed');
```

### Python
```python
import websocket
import json

def on_message(ws, message):
    result = json.loads(message)
    print(f"Result: {result}")

def on_error(ws, error):
    print(f"Error: {error}")

def on_close(ws, close_status_code, close_msg):
    print("Connection closed")

ws = websocket.WebSocketApp(
    "ws://localhost:8080/api/queries/test_query/results",
    on_message=on_message,
    on_error=on_error,
    on_close=on_close
)

ws.run_forever()
```

## 5. Run the Complete Example

```bash
# Terminal 1: Start the server
cargo run --bin http_server

# Terminal 2: Run the example client
cargo run --example http_client_example
```

The example demonstrates:
- Registering queries
- Starting/stopping replay
- Starting/stopping queries
- WebSocket result streaming
- All API endpoints

## API Endpoints Summary

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/health` | Health check |
| `POST` | `/api/queries` | Register a query |
| `GET` | `/api/queries` | List all queries |
| `GET` | `/api/queries/:id` | Get query details |
| `POST` | `/api/queries/:id/start` | Start query |
| `DELETE` | `/api/queries/:id` | Stop query |
| `WS` | `/api/queries/:id/results` | Stream results |
| `POST` | `/api/replay/start` | Start replay |
| `POST` | `/api/replay/stop` | Stop replay |
| `GET` | `/api/replay/status` | Replay status |

## Common Workflows

### Workflow 1: Historical Data Analysis
```bash
# 1. Start server
cargo run --bin http_server

# 2. Load data into storage
curl -X POST http://localhost:8080/api/replay/start \
  -H "Content-Type: application/json" \
  -d '{"input_file": "data/sensors.nq", "broker_type": "none", "rate_of_publishing": 10000}'

# 3. Wait for data ingestion (check status)
curl http://localhost:8080/api/replay/status

# 4. Register and start query
curl -X POST http://localhost:8080/api/queries \
  -H "Content-Type: application/json" \
  -d '{"query_id": "analysis", "janusql": "SELECT ?sensor ?temp FROM HISTORICAL FIXED WINDOW [2024-01-01T00:00:00Z, 2024-12-31T23:59:59Z] WHERE { ?sensor <http://example.org/temperature> ?temp . FILTER(?temp > 25.0) }"}'

curl -X POST http://localhost:8080/api/queries/analysis/start

# 5. Connect WebSocket to get results
# (Use browser console or WebSocket client)
```

### Workflow 2: Live Stream Processing
```bash
# 1. Register live query
curl -X POST http://localhost:8080/api/queries \
  -H "Content-Type: application/json" \
  -d '{"query_id": "live_monitor", "janusql": "SELECT ?sensor ?temp FROM LIVE SLIDING WINDOW sensors [RANGE PT10S, SLIDE PT5S] WHERE { ?sensor <http://example.org/temperature> ?temp . }"}'

# 2. Start query (before replay to catch all events)
curl -X POST http://localhost:8080/api/queries/live_monitor/start

# 3. Start replay with looping for continuous stream
curl -X POST http://localhost:8080/api/replay/start \
  -H "Content-Type: application/json" \
  -d '{"input_file": "data/sensors.nq", "broker_type": "none", "rate_of_publishing": 100, "loop_file": true}'

# 4. Connect WebSocket to stream live results
```

### Workflow 3: Hybrid (Historical + Live)
```bash
# Register hybrid query
curl -X POST http://localhost:8080/api/queries \
  -H "Content-Type: application/json" \
  -d '{"query_id": "hybrid", "janusql": "SELECT ?s ?p ?o FROM HISTORICAL FIXED WINDOW [2024-01-01T00:00:00Z, 2024-01-02T00:00:00Z] FROM LIVE SLIDING WINDOW stream [RANGE PT30S, SLIDE PT10S] WHERE { ?s ?p ?o }"}'

# Start replay first to populate historical data
curl -X POST http://localhost:8080/api/replay/start \
  -H "Content-Type: application/json" \
  -d '{"input_file": "data/sensors.nq", "broker_type": "none", "rate_of_publishing": 5000, "loop_file": true}'

# Start query - will process historical first, then live
curl -X POST http://localhost:8080/api/queries/hybrid/start

# WebSocket will receive both historical and live results
# Results tagged with "source": "historical" or "source": "live"
```

## Troubleshooting

### Server won't start
```bash
# Check if port 8080 is already in use
lsof -i :8080

# Use a different port
cargo run --bin http_server -- --port 8081
```

### No results from query
- Ensure replay is running: `curl http://localhost:8080/api/replay/status`
- Check query syntax is valid
- Verify data file exists and is valid N-Quads format
- Check server logs for errors

### WebSocket connection fails
- Ensure query is registered AND started before connecting
- Check browser console for CORS errors
- Verify WebSocket URL matches the query ID
- Try `ws://` not `wss://` for local testing

### Data not persisting
- Check storage directory exists and is writable
- Verify `--storage-dir` path is correct
- Check disk space availability

## Next Steps

1. Read the full [HTTP API Documentation](HTTP_API.md)
2. Learn [JanusQL Query Language](JANUSQL.md)
3. Explore [Stream Bus Configuration](STREAM_BUS.md)
4. Review [Architecture Overview](ARCHITECTURE.md)
5. Check [Benchmark Results](BENCHMARK_RESULTS.md)

## Example Data Format

If you need test data, create `data/sensors.nq`:

```nquads
<http://example.org/sensor1> <http://example.org/temperature> "23.5"^^<http://www.w3.org/2001/XMLSchema#double> <http://example.org/graph1> .
<http://example.org/sensor1> <http://example.org/timestamp> "2024-01-01T12:00:00Z"^^<http://www.w3.org/2001/XMLSchema#dateTime> <http://example.org/graph1> .
<http://example.org/sensor2> <http://example.org/temperature> "26.8"^^<http://www.w3.org/2001/XMLSchema#double> <http://example.org/graph1> .
<http://example.org/sensor2> <http://example.org/timestamp> "2024-01-01T12:00:01Z"^^<http://www.w3.org/2001/XMLSchema#dateTime> <http://example.org/graph1> .
```

## Support

- GitHub Issues: https://github.com/SolidLabResearch/janus/issues
- Documentation: See `HTTP_API.md` for complete API reference