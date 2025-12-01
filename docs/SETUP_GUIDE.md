# Janus HTTP API - Complete Setup Guide

This guide will walk you through setting up Janus with MQTT for both historical and live stream processing.

## Prerequisites

- Rust 1.70+ (`rustup update`)
- Docker and Docker Compose (for MQTT broker)
- Git

## Quick Start (5 minutes)

### Step 1: Start MQTT Broker

```bash
# Navigate to janus directory
cd janus

# Start Mosquitto MQTT broker with Docker Compose
docker-compose up -d

# Verify MQTT is running
docker-compose ps
```

Expected output:
```
NAME                  STATUS    PORTS
janus-mosquitto       Up        0.0.0.0:1883->1883/tcp, 0.0.0.0:9001->9001/tcp
```

### Step 2: Start Janus HTTP Server

```bash
# In the janus directory
cargo run --bin http_server

# Server will start on http://127.0.0.1:8080
```

### Step 3: Open Demo Dashboard

```bash
# Open in your default browser
open examples/demo_dashboard.html

# Or manually navigate to:
# file:///path/to/janus/examples/demo_dashboard.html
```

### Step 4: Test the System

1. **Click "Start Replay"** button
   - Loads data from `data/sensors.nq`
   - Publishes to MQTT topic `sensors`
   - Stores in local storage
   - Watch the metrics update in real-time

2. **Click "Start Query"** button
   - Registers and starts a historical query
   - Connects WebSocket for results
   - Watch results appear in the panel below

## Detailed Setup

### 1. Clone and Build

```bash
# Clone the repository
git clone https://github.com/SolidLabResearch/janus.git
cd janus

# Build the project
cargo build --release

# Verify build
./target/release/http_server --help
```

### 2. MQTT Broker Setup

#### Option A: Docker Compose (Recommended)

```bash
# Start MQTT broker
docker-compose up -d mosquitto

# Check logs
docker-compose logs -f mosquitto

# Stop when done
docker-compose down
```

#### Option B: Local Mosquitto Installation

**macOS:**
```bash
brew install mosquitto
mosquitto -c /usr/local/etc/mosquitto/mosquitto.conf
```

**Linux (Ubuntu/Debian):**
```bash
sudo apt-get install mosquitto mosquitto-clients
sudo systemctl start mosquitto
sudo systemctl enable mosquitto
```

**Windows:**
Download from https://mosquitto.org/download/

Configuration file (`mosquitto.conf`):
```conf
listener 1883
allow_anonymous true
```

#### Option C: Public MQTT Broker (Testing Only)

You can use a public broker for testing:
- `test.mosquitto.org:1883`
- `broker.hivemq.com:1883`

**Note:** Public brokers are NOT recommended for production or sensitive data.

### 3. Prepare Test Data

Create `data/sensors.nq` with sample RDF data:

```bash
mkdir -p data

cat > data/sensors.nq << 'EOF'
<http://example.org/sensor1> <http://example.org/temperature> "23.5"^^<http://www.w3.org/2001/XMLSchema#double> <http://example.org/graph1> .
<http://example.org/sensor1> <http://example.org/timestamp> "2024-01-01T12:00:00Z"^^<http://www.w3.org/2001/XMLSchema#dateTime> <http://example.org/graph1> .
<http://example.org/sensor2> <http://example.org/temperature> "26.8"^^<http://www.w3.org/2001/XMLSchema#double> <http://example.org/graph1> .
<http://example.org/sensor2> <http://example.org/timestamp> "2024-01-01T12:00:01Z"^^<http://www.w3.org/2001/XMLSchema#dateTime> <http://example.org/graph1> .
<http://example.org/sensor3> <http://example.org/temperature> "21.2"^^<http://www.w3.org/2001/XMLSchema#double> <http://example.org/graph1> .
<http://example.org/sensor3> <http://example.org/timestamp> "2024-01-01T12:00:02Z"^^<http://www.w3.org/2001/XMLSchema#dateTime> <http://example.org/graph1> .
EOF
```

### 4. Start HTTP Server

```bash
# Default configuration (localhost:8080)
cargo run --bin http_server

# Custom configuration
cargo run --bin http_server -- \
  --host 0.0.0.0 \
  --port 8080 \
  --storage-dir ./data/storage \
  --max-batch-size-bytes 10485760 \
  --flush-interval-ms 5000
```

Server options:
- `--host`: Bind address (default: 127.0.0.1)
- `--port`: Server port (default: 8080)
- `--storage-dir`: Storage directory (default: ./data/storage)
- `--max-batch-size-bytes`: Max batch size before flush (default: 10MB)
- `--flush-interval-ms`: Flush interval in milliseconds (default: 5000ms)

### 5. Verify Setup

#### Test MQTT Broker

```bash
# Terminal 1: Subscribe to test topic
docker exec -it janus-mosquitto mosquitto_sub -t "sensors" -v

# Terminal 2: Publish test message
docker exec -it janus-mosquitto mosquitto_pub -t "sensors" -m "test message"
```

You should see "test message" in Terminal 1.

#### Test HTTP Server

```bash
# Health check
curl http://localhost:8080/health

# Should return: {"message":"Janus HTTP API is running"}
```

## Usage Workflows

### Workflow 1: Historical Query Only

```bash
# Terminal 1: Start server
cargo run --bin http_server

# Terminal 2: Register historical query
curl -X POST http://localhost:8080/api/queries \
  -H "Content-Type: application/json" \
  -d '{
    "query_id": "historical_temps",
    "janusql": "PREFIX ex: <http://example.org/> REGISTER RStream ex:output AS SELECT ?sensor ?temp FROM NAMED WINDOW ex:histWindow ON STREAM ex:sensorStream [START 1704067200 END 1735689599] WHERE { WINDOW ex:histWindow { ?sensor ex:temperature ?temp . } }"
  }'

# Start replay (to populate storage)
curl -X POST http://localhost:8080/api/replay/start \
  -H "Content-Type: application/json" \
  -d '{
    "input_file": "data/sensors.nq",
    "broker_type": "none",
    "topics": ["sensors"],
    "rate_of_publishing": 5000
  }'

# Wait a few seconds, then start query
curl -X POST http://localhost:8080/api/queries/historical_temps/start

# Connect WebSocket to get results (use browser console or websocket client)
```

### Workflow 2: Live Stream Processing

```bash
# Ensure MQTT is running
docker-compose up -d mosquitto

# Register live query
curl -X POST http://localhost:8080/api/queries \
  -H "Content-Type: application/json" \
  -d '{
    "query_id": "live_temps",
    "janusql": "PREFIX ex: <http://example.org/> REGISTER RStream ex:output AS SELECT ?sensor ?temp FROM NAMED WINDOW ex:liveWindow ON STREAM ex:sensorStream [RANGE 10000 STEP 5000] WHERE { WINDOW ex:liveWindow { ?sensor ex:temperature ?temp . } }"
  }'

# Start query (before replay to catch all events)
curl -X POST http://localhost:8080/api/queries/live_temps/start

# Start replay with MQTT
curl -X POST http://localhost:8080/api/replay/start \
  -H "Content-Type: application/json" \
  -d '{
    "input_file": "data/sensors.nq",
    "broker_type": "mqtt",
    "topics": ["sensors"],
    "rate_of_publishing": 100,
    "loop_file": true,
    "mqtt_config": {
      "host": "localhost",
      "port": 1883,
      "client_id": "janus_client",
      "keep_alive_secs": 30
    }
  }'

# Results will stream via WebSocket at ws://localhost:8080/api/queries/live_temps/results
```

### Workflow 3: Hybrid (Historical + Live)

```bash
# Register hybrid query
curl -X POST http://localhost:8080/api/queries \
  -H "Content-Type: application/json" \
  -d '{
    "query_id": "hybrid_analysis",
    "janusql": "PREFIX ex: <http://example.org/> REGISTER RStream ex:output AS SELECT ?sensor ?temp FROM NAMED WINDOW ex:histWindow ON STREAM ex:sensorStream [START 1704067200 END 1704153599] FROM NAMED WINDOW ex:liveWindow ON STREAM ex:sensorStream [RANGE 30000 STEP 10000] WHERE { WINDOW ex:histWindow { ?sensor ex:temperature ?temp . } WINDOW ex:liveWindow { ?sensor ex:temperature ?temp . } }"
  }'

# Start replay with MQTT
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
      "client_id": "janus_hybrid",
      "keep_alive_secs": 30
    }
  }'

# Wait for data to load into storage
sleep 5

# Start query - will process historical first, then live
curl -X POST http://localhost:8080/api/queries/hybrid_analysis/start

# WebSocket will receive:
# - Historical results tagged with "source": "historical"
# - Live results tagged with "source": "live"
```

## Monitoring and Debugging

### Monitor MQTT Messages

```bash
# Subscribe to all topics
docker exec -it janus-mosquitto mosquitto_sub -t "#" -v

# Subscribe to specific topic
docker exec -it janus-mosquitto mosquitto_sub -t "sensors" -v
```

### Check Replay Status

```bash
curl http://localhost:8080/api/replay/status | jq
```

### Check Query Status

```bash
curl http://localhost:8080/api/queries/your_query_id | jq
```

### View Server Logs

```bash
# Server logs are printed to stdout
# Look for:
# - "Janus HTTP API server listening on..."
# - Query registration confirmations
# - Error messages
```

### MQTT Broker Logs

```bash
docker-compose logs -f mosquitto
```

## Troubleshooting

### MQTT Broker Won't Start

**Check if port 1883 is in use:**
```bash
lsof -i :1883
```

**Solution:** Kill the process or use a different port
```bash
# Edit docker-compose.yml to change port
# Then restart
docker-compose down
docker-compose up -d
```

### No Data in MQTT Topic

**Verify replay is publishing to MQTT:**
```bash
# Check replay status
curl http://localhost:8080/api/replay/status

# Should show broker_type: "mqtt"
```

**Subscribe to topic to verify messages:**
```bash
docker exec -it janus-mosquitto mosquitto_sub -t "sensors" -v
```

### Live Query Not Receiving Events

**Checklist:**
1. MQTT broker is running: `docker-compose ps`
2. Replay is using `broker_type: "mqtt"`
3. Query is started BEFORE replay (or replay is looping)
4. MQTT topic matches the query's stream name
5. Live window specification is correct

**Debug steps:**
```bash
# 1. Verify MQTT messages
docker exec -it janus-mosquitto mosquitto_sub -t "sensors" -v

# 2. Check query status
curl http://localhost:8080/api/queries/your_query_id

# 3. Check server logs for errors
```

### WebSocket Connection Fails

**Checklist:**
1. Query is registered: `GET /api/queries`
2. Query is started: `POST /api/queries/:id/start`
3. Browser allows WebSocket connections
4. Correct WebSocket URL: `ws://localhost:8080/api/queries/:id/results`

**Test WebSocket with browser console:**
```javascript
const ws = new WebSocket('ws://localhost:8080/api/queries/your_query_id/results');
ws.onopen = () => console.log('Connected');
ws.onmessage = (e) => console.log('Message:', JSON.parse(e.data));
ws.onerror = (e) => console.error('Error:', e);
```

### Server Won't Start

**Port already in use:**
```bash
lsof -i :8080
# Use different port
cargo run --bin http_server -- --port 8081
```

**Build errors:**
```bash
# Clean and rebuild
cargo clean
cargo build --release
```

### No Results from Query

**Historical queries:**
- Ensure data is in storage (run replay first)
- Check time window matches your data timestamps
- Verify N-Quads file is valid

**Live queries:**
- Ensure MQTT broker is running
- Verify replay is publishing to MQTT
- Check query window specification

## Performance Tuning

### For High Throughput

```bash
cargo run --bin http_server -- \
  --max-batch-size-bytes 52428800 \
  --flush-interval-ms 1000
```

### For Low Latency

```bash
cargo run --bin http_server -- \
  --max-batch-size-bytes 1048576 \
  --flush-interval-ms 100
```

### MQTT Broker Tuning

Edit `docker/mosquitto/config/mosquitto.conf`:
```conf
max_connections 1000
max_queued_messages 10000
message_size_limit 0
```

Restart broker:
```bash
docker-compose restart mosquitto
```

## Production Deployment

### Security Checklist

- [ ] Add authentication to HTTP API
- [ ] Enable MQTT authentication
- [ ] Use HTTPS/WSS instead of HTTP/WS
- [ ] Restrict CORS to specific origins
- [ ] Add rate limiting
- [ ] Use firewall rules
- [ ] Enable SSL/TLS for MQTT

### MQTT with Authentication

Edit `docker/mosquitto/config/mosquitto.conf`:
```conf
allow_anonymous false
password_file /mosquitto/config/passwd
```

Create password file:
```bash
docker exec -it janus-mosquitto mosquitto_passwd -c /mosquitto/config/passwd username
docker-compose restart mosquitto
```

### Reverse Proxy (nginx)

```nginx
server {
    listen 80;
    server_name janus.example.com;

    location / {
        proxy_pass http://localhost:8080;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
    }
}
```

## Next Steps

1. Read the [HTTP API Documentation](HTTP_API.md)
2. Learn [JanusQL Query Language](JANUSQL.md)
3. Explore [Example Client](examples/http_client_example.rs)
4. Review [Architecture](ARCHITECTURE.md)
5. Check [Benchmark Results](BENCHMARK_RESULTS.md)

## Support

- GitHub Issues: https://github.com/SolidLabResearch/janus/issues
- Documentation: Complete API reference in `HTTP_API.md`

## Summary

You now have:
- ✅ MQTT broker running (Mosquitto)
- ✅ Janus HTTP server running
- ✅ Demo dashboard ready to use
- ✅ Sample data prepared
- ✅ Both historical and live processing capabilities

**Ready to process RDF streams!**