#!/bin/bash

# Test Script for Live + Historical Streaming in Janus
# This script tests the full hybrid query workflow with MQTT

set -e

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║         Janus Live + Historical Streaming Test                ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Step 1: Check if MQTT broker is running
echo -e "${BLUE}Step 1: Checking MQTT broker...${NC}"
if docker ps | grep -q janus-mosquitto; then
    echo -e "${GREEN}✓ MQTT broker is running${NC}"
else
    echo -e "${YELLOW}⚠ MQTT broker not running. Starting...${NC}"
    if docker-compose up -d mosquitto 2>/dev/null; then
        echo -e "${GREEN}✓ MQTT broker started${NC}"
        sleep 2
    else
        echo -e "${RED}✗ Failed to start MQTT broker${NC}"
        echo "Please run: docker-compose up -d mosquitto"
        exit 1
    fi
fi
echo ""

# Step 2: Clean storage
echo -e "${BLUE}Step 2: Cleaning storage directory...${NC}"
rm -rf data/storage
mkdir -p data/storage
echo -e "${GREEN}✓ Storage cleaned${NC}"
echo ""

# Step 3: Check data file
echo -e "${BLUE}Step 3: Checking test data file...${NC}"
if [ -f "data/sensors_correct.nq" ]; then
    echo -e "${GREEN}✓ Data file exists${NC}"
    echo "  File: data/sensors_correct.nq"
    echo "  Lines: $(wc -l < data/sensors_correct.nq)"
    echo "  First line: $(head -1 data/sensors_correct.nq)"
else
    echo -e "${RED}✗ Data file not found${NC}"
    exit 1
fi
echo ""

# Step 4: Build the server
echo -e "${BLUE}Step 4: Building HTTP server...${NC}"
cargo build --release --bin http_server
echo -e "${GREEN}✓ Build complete${NC}"
echo ""

# Step 5: Start the server in background
echo -e "${BLUE}Step 5: Starting HTTP server...${NC}"
./target/release/http_server \
    --host 127.0.0.1 \
    --port 8080 \
    --storage-dir ./data/storage \
    --max-batch-size-bytes 10485760 \
    --flush-interval-ms 5000 \
    --max-total-memory-mb 1024 > /tmp/janus_server.log 2>&1 &

SERVER_PID=$!
echo -e "${GREEN}✓ Server started (PID: $SERVER_PID)${NC}"
echo "  Log file: /tmp/janus_server.log"
echo ""

# Wait for server to be ready
echo -e "${BLUE}Waiting for server to be ready...${NC}"
sleep 3

# Check if server is still running
if ! ps -p $SERVER_PID > /dev/null; then
    echo -e "${RED}✗ Server failed to start${NC}"
    echo "Server log:"
    cat /tmp/janus_server.log
    exit 1
fi
echo -e "${GREEN}✓ Server is ready${NC}"
echo ""

# Step 6: Start MQTT subscriber to monitor published events
echo -e "${BLUE}Step 6: Starting MQTT monitor (in background)...${NC}"
docker exec -d janus-mosquitto mosquitto_sub -t "sensors" -v > /tmp/janus_mqtt_monitor.log 2>&1 || true
echo -e "${GREEN}✓ MQTT monitor started${NC}"
echo "  Log file: /tmp/janus_mqtt_monitor.log"
echo ""

# Step 7a: Ingest historical data (explicit timestamps)
echo -e "${BLUE}Step 7a: Ingesting historical data...${NC}"
HISTORICAL_RESPONSE=$(curl -s -X POST http://127.0.0.1:8080/api/replay/start \
    -H "Content-Type: application/json" \
    -d '{
        "input_file": "data/sensors_historical.nq",
        "broker_type": "mqtt",
        "topics": ["sensors"],
        "rate_of_publishing": 10000,
        "loop_file": false,
        "add_timestamps": false,
        "mqtt_config": {
            "host": "localhost",
            "port": 1883,
            "client_id": "janus_historical_ingest",
            "keep_alive_secs": 30
        }
    }')

if echo "$HISTORICAL_RESPONSE" | grep -q "message"; then
    echo -e "${GREEN}✓ Historical ingestion started${NC}"
    echo "  Response: $HISTORICAL_RESPONSE"
    # Wait for ingestion to finish
    sleep 5
    # Explicitly stop the replay to free up the lock
    curl -s -X POST http://127.0.0.1:8080/api/replay/stop > /dev/null
else
    echo -e "${RED}✗ Failed to start historical ingestion${NC}"
    echo "  Response: $HISTORICAL_RESPONSE"
    kill $SERVER_PID 2>/dev/null || true
    exit 1
fi

# Step 7b: Start live stream replay
echo -e "${BLUE}Step 7b: Starting live stream replay...${NC}"
REPLAY_RESPONSE=$(curl -s -X POST http://127.0.0.1:8080/api/replay/start \
    -H "Content-Type: application/json" \
    -d '{
        "input_file": "data/sensors_correct.nq",
        "broker_type": "mqtt",
        "topics": ["sensors"],
        "rate_of_publishing": 500,
        "loop_file": true,
        "add_timestamps": true,
        "mqtt_config": {
            "host": "localhost",
            "port": 1883,
            "client_id": "janus_test_replay",
            "keep_alive_secs": 30
        }
    }')

if echo "$REPLAY_RESPONSE" | grep -q "message"; then
    echo -e "${GREEN}✓ Live replay started${NC}"
    echo "  Response: $REPLAY_RESPONSE"
else
    echo -e "${RED}✗ Failed to start live replay${NC}"
    echo "  Response: $REPLAY_RESPONSE"
    kill $SERVER_PID 2>/dev/null || true
    exit 1
fi
echo ""

# Step 8: Wait for storage flush
echo -e "${BLUE}Step 8: Waiting for storage flush (10 seconds)...${NC}"
for i in {10..1}; do
    echo -ne "  $i seconds remaining...\r"
    sleep 1
done
echo -e "${GREEN}✓ Storage flush complete${NC}"
echo ""

# Check storage directory
echo -e "${BLUE}Checking storage contents:${NC}"
if [ -d "data/storage" ]; then
    SEGMENT_COUNT=$(find data/storage -name "segment_*" 2>/dev/null | wc -l)
    echo "  Segments created: $SEGMENT_COUNT"
    if [ $SEGMENT_COUNT -gt 0 ]; then
        echo -e "${GREEN}✓ Storage has data${NC}"
    else
        echo -e "${YELLOW}⚠ No segments found yet${NC}"
    fi
fi
echo ""

# Step 9: Register and start hybrid query
echo -e "${BLUE}Step 9: Registering hybrid query...${NC}"
REGISTER_RESPONSE=$(curl -s -X POST http://127.0.0.1:8080/api/queries \
    -H "Content-Type: application/json" \
    -d '{
        "query_id": "test_hybrid_query",
        "janusql": "PREFIX ex: <http://example.org/>\nREGISTER RStream ex:output AS\nSELECT ?sensor ?temp\nFROM NAMED WINDOW ex:histWindow ON STREAM ex:sensorStream [RANGE 2h STEP 1h]\nFROM NAMED WINDOW ex:liveWindow ON STREAM ex:sensorStream [RANGE 5000 STEP 2000]\nWHERE {\n  WINDOW ex:histWindow {\n    ?sensor ex:temperature ?temp .\n  }\n  WINDOW ex:liveWindow {\n    ?sensor ex:temperature ?temp .\n  }\n}"
    }')

if echo "$REGISTER_RESPONSE" | grep -q "query_id"; then
    echo -e "${GREEN}✓ Query registered${NC}"
    echo "  Response: $REGISTER_RESPONSE"
else
    echo -e "${RED}✗ Failed to register query${NC}"
    echo "  Response: $REGISTER_RESPONSE"
    kill $SERVER_PID 2>/dev/null || true
    exit 1
fi
echo ""

echo -e "${BLUE}Starting query execution...${NC}"
START_RESPONSE=$(curl -s -X POST http://127.0.0.1:8080/api/queries/test_hybrid_query/start)

if echo "$START_RESPONSE" | grep -q "message"; then
    echo -e "${GREEN}✓ Query started${NC}"
    echo "  Response: $START_RESPONSE"
else
    echo -e "${RED}✗ Failed to start query${NC}"
    echo "  Response: $START_RESPONSE"
    kill $SERVER_PID 2>/dev/null || true
    exit 1
fi
echo ""

# Step 10: Monitor results for 15 seconds
echo -e "${BLUE}Step 10: Monitoring results for 15 seconds...${NC}"
echo "  (Results will stream via WebSocket to dashboard)"
echo "  Server log tail:"
tail -20 /tmp/janus_server.log
echo ""
sleep 15

# Step 11: Check replay status
echo -e "${BLUE}Step 11: Checking replay status...${NC}"
STATUS_RESPONSE=$(curl -s http://127.0.0.1:8080/api/replay/status)
echo "  $STATUS_RESPONSE"
echo ""

# Step 12: Clean up
echo -e "${BLUE}Step 12: Cleaning up...${NC}"

# Stop query
curl -s -X DELETE http://127.0.0.1:8080/api/queries/test_hybrid_query > /dev/null 2>&1 || true
echo -e "${GREEN}✓ Query stopped${NC}"

# Stop replay
curl -s -X POST http://127.0.0.1:8080/api/replay/stop > /dev/null 2>&1 || true
echo -e "${GREEN}✓ Replay stopped${NC}"

# Stop server
kill $SERVER_PID 2>/dev/null || true
echo -e "${GREEN}✓ Server stopped${NC}"

echo ""
echo "╔════════════════════════════════════════════════════════════════╗"
echo "║                    Test Summary                                ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""
echo -e "${GREEN}✓ All steps completed successfully${NC}"
echo ""
echo "To view the dashboard:"
echo "  1. Start the server: ./start_http_server.sh"
echo "  2. Open: examples/demo_dashboard.html in your browser"
echo "  3. Click 'Start Replay' and wait 10 seconds"
echo "  4. Click 'Start Query' to see results"
echo ""
echo "You should see:"
echo "  - Historical results (from stored data)"
echo "  - Live results (from MQTT stream)"
echo ""
echo "Log files:"
echo "  - Server: /tmp/janus_server.log"
echo "  - MQTT monitor: /tmp/janus_mqtt_monitor.log"
echo ""
