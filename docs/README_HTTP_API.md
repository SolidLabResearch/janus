# Janus HTTP API - Complete Guide

> Unified Live and Historical RDF Stream Processing via HTTP/WebSocket

## Overview

The Janus HTTP API provides REST endpoints and WebSocket streaming for managing and executing RDF stream queries. It supports both historical data analysis and live stream processing through a unified interface.

## Quick Start (3 Steps)

### 1. Start MQTT Broker

```bash
docker-compose up -d mosquitto
```

### 2. Start HTTP Server

```bash
cargo run --bin http_server
```

### 3. Open Demo Dashboard

```bash
open examples/demo_dashboard.html
```

Click "Start Replay" then "Start Query" to see live results.

## Complete Setup

### Prerequisites

- Rust 1.70+
- Docker & Docker Compose
- Sample data file (provided)

### Installation

```bash
# Clone repository
git clone https://github.com/SolidLabResearch/janus.git
cd janus

# Run automated setup
./scripts/test_setup.sh

# Start HTTP server (in new terminal)
cargo run --bin http_server

# Open dashboard
open examples/demo_dashboard.html
```

## JanusQL Query Syntax

### Historical Query

```sparql
PREFIX ex: <http://example.org/>
REGISTER RStream ex:output AS
SELECT ?sensor ?temp ?time
FROM NAMED WINDOW ex:histWindow ON STREAM ex:sensorStream [START 1704067200 END 1735689599]
WHERE {
  WINDOW ex:histWindow {
    ?sensor ex:temperature ?temp .
    ?sensor ex:timestamp ?time .
  }
}
```

### Live Query

```sparql
PREFIX ex: <http://example.org/>
REGISTER RStream ex:output AS
SELECT ?sensor ?temp
FROM NAMED WINDOW ex:liveWindow ON STREAM ex:sensorStream [RANGE 10000 STEP 5000]
WHERE {
  WINDOW ex:liveWindow {
    ?sensor ex:temperature ?temp .
  }
}
```

### Hybrid Query (Historical + Live)

```sparql
PREFIX ex: <http://example.org/>
REGISTER RStream ex:output AS
SELECT ?sensor ?temp
FROM NAMED WINDOW ex:histWindow ON STREAM ex:sensorStream [START 1704067200 END 1704153599]
FROM NAMED WINDOW ex:liveWindow ON STREAM ex:sensorStream [RANGE 30000 STEP 10000]
WHERE {
  WINDOW ex:histWindow {
    ?sensor ex:temperature ?temp .
  }
  WINDOW ex:liveWindow {
    ?sensor ex:temperature ?temp .
  }
}
```

## HTTP API Endpoints

### Query Management

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/api/queries` | Register a query |
| GET | `/api/queries` | List all queries |
| GET | `/api/queries/:id` | Get query details |
| POST | `/api/queries/:id/start` | Start query execution |
| DELETE | `/api/queries/:id` | Stop query |
| WS | `/api/queries/:id/results` | Stream results |

### Stream Replay

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/api/replay/start` | Start data replay |
| POST | `/api/replay/stop` | Stop replay |
| GET | `/api/replay/status` | Get replay metrics |

## Usage Examples

### Register and Start Query

```bash
# Register query
curl -X POST http://localhost:8080/api/queries \
  -H "Content-Type: application/json" \
  -d '{
    "query_id": "sensor_analysis",
    "janusql": "PREFIX ex: <http://example.org/> REGISTER RStream ex:output AS SELECT ?sensor ?temp FROM NAMED WINDOW ex:histWindow ON STREAM ex:sensorStream [START 1704067200 END 1735689599] WHERE { WINDOW ex:histWindow { ?sensor ex:temperature ?temp . } }"
  }'

# Start query
curl -X POST http://localhost:8080/api/queries/sensor_analysis/start
```

### Start Replay with MQTT

```bash
curl -X POST http://localhost:8080/api/replay/start \
  -H "Content-Type: application/json" \
  -d '{
    "input_file": "data/sensors.nq",
    "broker_type": "mqtt",
    "topics": ["sensors"],
    "rate_of_publishing": 1000,
    "loop_file": true,
    "mqtt_config": {
      "host": "localhost",
      "port": 1883,
      "client_id": "janus_client",
      "keep_alive_secs": 30
    }
  }'
```

### WebSocket Streaming (JavaScript)

```javascript
const ws = new WebSocket('ws://localhost:8080/api/queries/sensor_analysis/results');

ws.onmessage = (event) => {
  const result = JSON.parse(event.data);
  console.log('Source:', result.source);      // "historical" or "live"
  console.log('Timestamp:', result.timestamp);
  console.log('Bindings:', result.bindings);
};
```

## Architecture

### Components

```
┌─────────────────┐
│  Web Dashboard  │
│   (Browser)     │
└────────┬────────┘
         │ HTTP/WebSocket
         ▼
┌─────────────────┐
│   HTTP Server   │
│  (Axum/Tokio)   │
└────────┬────────┘
         │
    ┌────┴─────┐
    │          │
    ▼          ▼
┌─────────┐ ┌──────────┐
│ Storage │ │ JanusAPI │
└─────────┘ └─────┬────┘
                  │
         ┌────────┴────────┐
         │                 │
         ▼                 ▼
    ┌──────────┐    ┌──────────┐
    │Historical│    │   Live   │
    │ Executor │    │Processor │
    └──────────┘    └─────┬────┘
                          │
                          ▼
                    ┌──────────┐
                    │   MQTT   │
                    │  Broker  │
                    └──────────┘
```

### Data Flow

1. **Historical Processing**:
   - Data loaded into storage via replay
   - Query executes against stored data
   - Results returned via WebSocket

2. **Live Processing**:
   - Data published to MQTT topic
   - Live processor subscribes to topic
   - Results streamed in real-time via WebSocket

3. **Hybrid Processing**:
   - Historical results sent first
   - Live results streamed continuously
   - All tagged with source type

## Configuration

### Server Options

```bash
cargo run --bin http_server -- \
  --host 0.0.0.0 \
  --port 8080 \
  --storage-dir ./data/storage \
  --max-batch-size-bytes 10485760 \
  --flush-interval-ms 5000
```

### MQTT Configuration

Edit `docker/mosquitto/config/mosquitto.conf`:

```conf
listener 1883
allow_anonymous true
persistence true
persistence_location /mosquitto/data/
```

## Troubleshooting

### MQTT Broker Issues

```bash
# Check if running
docker ps | grep mosquitto

# View logs
docker-compose logs -f mosquitto

# Restart
docker-compose restart mosquitto
```

### No Live Query Results

**Checklist:**
1. MQTT broker is running
2. Replay using `broker_type: "mqtt"`
3. Query started before replay (or replay is looping)
4. MQTT topic matches stream name in query

**Debug:**
```bash
# Subscribe to MQTT topic to verify messages
docker exec -it janus-mosquitto mosquitto_sub -t "sensors" -v
```

### WebSocket Connection Fails

**Checklist:**
1. Query is registered: `GET /api/queries`
2. Query is started: `POST /api/queries/:id/start`
3. Correct URL: `ws://localhost:8080/api/queries/:id/results`

**Test in browser console:**
```javascript
const ws = new WebSocket('ws://localhost:8080/api/queries/your_id/results');
ws.onopen = () => console.log('Connected');
ws.onerror = (e) => console.error('Error:', e);
```

## Demo Dashboard Features

The interactive dashboard (`examples/demo_dashboard.html`) provides:

- **Start Replay** - Begins data ingestion with MQTT publishing
- **Start Query** - Executes query and streams results
- **Real-time Metrics** - Events read, stored, processing rate
- **Live Results** - Color-coded historical vs. live results
- **Status Monitoring** - Connection status, error handling

## Example Client

Run the complete example demonstrating all endpoints:

```bash
cargo run --example http_client_example
```

This demonstrates:
- Health check
- Query registration
- Query lifecycle management
- Stream replay control
- WebSocket result streaming
- Error handling

## Performance

### Benchmarks

- **Write Throughput**: 2.6-3.14 Million quads/sec
- **Query Latency**: Sub-millisecond point queries
- **Compression**: 40% reduction (40 bytes → 24 bytes per quad)
- **WebSocket**: Low-latency streaming (<10ms)

### Tuning

**High Throughput:**
```bash
cargo run --bin http_server -- \
  --max-batch-size-bytes 52428800 \
  --flush-interval-ms 1000
```

**Low Latency:**
```bash
cargo run --bin http_server -- \
  --max-batch-size-bytes 1048576 \
  --flush-interval-ms 100
```

## Production Deployment

### Security Recommendations

- Add authentication (JWT, OAuth2)
- Enable HTTPS/WSS
- Restrict CORS origins
- Add rate limiting
- Enable MQTT authentication
- Use firewall rules

### Example nginx Configuration

```nginx
server {
    listen 443 ssl;
    server_name janus.example.com;

    ssl_certificate /path/to/cert.pem;
    ssl_certificate_key /path/to/key.pem;

    location / {
        proxy_pass http://localhost:8080;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
    }
}
```

## Documentation

- **[SETUP_GUIDE.md](SETUP_GUIDE.md)** - Detailed setup instructions
- **[HTTP_API_IMPLEMENTATION.md](HTTP_API_IMPLEMENTATION.md)** - Implementation details
- **[ARCHITECTURE.md](ARCHITECTURE.md)** - System architecture
- **[BENCHMARK_RESULTS.md](BENCHMARK_RESULTS.md)** - Performance metrics

## Testing

```bash
# Run tests
cargo test

# Build and run server
cargo run --bin http_server

# Run example client
cargo run --example http_client_example

# Format code
make fmt

# Lint
make clippy
```

## Common Workflows

### Analyze Historical Data

1. Start server and MQTT
2. Load data: `POST /api/replay/start` (broker_type: "none" or "mqtt")
3. Register query: `POST /api/queries`
4. Start query: `POST /api/queries/:id/start`
5. Connect WebSocket for results

### Process Live Streams

1. Start server and MQTT
2. Register live query: `POST /api/queries`
3. Start query: `POST /api/queries/:id/start`
4. Start replay: `POST /api/replay/start` (broker_type: "mqtt", loop_file: true)
5. Receive live results via WebSocket

### Hybrid Analysis

1. Start server and MQTT
2. Register hybrid query (historical + live windows)
3. Start replay with MQTT
4. Wait for historical data to load
5. Start query
6. Receive historical results, then live results (both tagged)

## Support

- **GitHub**: https://github.com/SolidLabResearch/janus
- **Issues**: https://github.com/SolidLabResearch/janus/issues
- **Documentation**: Complete API docs in `docs/` directory

## License

MIT

## Citation

If you use Janus in your research, please cite:

```bibtex
@software{janus2024,
  title = {Janus: Unified Live and Historical RDF Stream Processing},
  author = {Bisen, Kush},
  year = {2024},
  url = {https://github.com/SolidLabResearch/janus}
}
```

## Contributors

See [CONTRIBUTORS.md](CONTRIBUTORS.md)

---

**Ready to process RDF streams!** 🚀

For questions or issues, please open a GitHub issue or refer to the comprehensive documentation in the `docs/` directory.