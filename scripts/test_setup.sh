#!/bin/bash

# Janus HTTP API Test Script

set -e

# Ensure we are in the project root
cd "$(dirname "$0")/.."

echo "🔧 Janus HTTP API - Complete Setup Test"
echo "========================================"
echo ""

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Step 1: Check Docker
echo "1. Checking Docker..."
if ! command -v docker &> /dev/null; then
    echo -e "${RED}✗ Docker not found. Please install Docker first.${NC}"
    exit 1
fi
echo -e "${GREEN}✓ Docker found${NC}"

# Step 2: Check Docker Compose
echo "2. Checking Docker Compose..."
if ! command -v docker-compose &> /dev/null; then
    echo -e "${RED}✗ Docker Compose not found. Please install Docker Compose first.${NC}"
    exit 1
fi
echo -e "${GREEN}✓ Docker Compose found${NC}"

# Step 3: Start MQTT Broker
echo "3. Starting MQTT broker..."
docker-compose up -d mosquitto 2>/dev/null || {
    echo -e "${YELLOW}⚠ Could not start via docker-compose, trying docker directly...${NC}"
    docker run -d --name janus-mosquitto -p 1883:1883 -p 9001:9001 eclipse-mosquitto:2.0
}

# Wait for MQTT to be ready
sleep 2

# Check if MQTT is running
if docker ps | grep -q mosquitto; then
    echo -e "${GREEN}✓ MQTT broker running${NC}"
else
    echo -e "${RED}✗ MQTT broker failed to start${NC}"
    exit 1
fi

# Step 4: Create test data
echo "4. Creating test data..."
mkdir -p data
cat > data/sensors.nq << 'NQUADS'
<http://example.org/sensor1> <http://example.org/temperature> "23.5"^^<http://www.w3.org/2001/XMLSchema#double> <http://example.org/graph1> .
<http://example.org/sensor1> <http://example.org/timestamp> "2024-01-01T12:00:00Z"^^<http://www.w3.org/2001/XMLSchema#dateTime> <http://example.org/graph1> .
<http://example.org/sensor2> <http://example.org/temperature> "26.8"^^<http://www.w3.org/2001/XMLSchema#double> <http://example.org/graph1> .
<http://example.org/sensor2> <http://example.org/timestamp> "2024-01-01T12:00:01Z"^^<http://www.w3.org/2001/XMLSchema#dateTime> <http://example.org/graph1> .
<http://example.org/sensor3> <http://example.org/temperature> "21.2"^^<http://www.w3.org/2001/XMLSchema#double> <http://example.org/graph1> .
<http://example.org/sensor3> <http://example.org/timestamp> "2024-01-01T12:00:02Z"^^<http://www.w3.org/2001/XMLSchema#dateTime> <http://example.org/graph1> .
NQUADS
echo -e "${GREEN}✓ Test data created in data/sensors.nq${NC}"

# Step 5: Build Janus
echo "5. Building Janus HTTP server..."
if cargo build --bin http_server 2>&1 | tail -5; then
    echo -e "${GREEN}✓ Janus built successfully${NC}"
else
    echo -e "${RED}✗ Build failed${NC}"
    exit 1
fi

echo ""
echo "========================================"
echo -e "${GREEN}✓ Setup Complete!${NC}"
echo ""
echo "Next steps:"
echo ""
echo "1. Start the HTTP server in a new terminal:"
echo "   cargo run --bin http_server"
echo ""
echo "2. Open the demo dashboard:"
echo "   open examples/demo_dashboard.html"
echo ""
echo "3. Click 'Start Replay' then 'Start Query'"
echo ""
echo "Or run the automated client example:"
echo "   cargo run --example http_client_example"
echo ""
echo "To stop MQTT broker:"
echo "   docker-compose down"
echo ""
