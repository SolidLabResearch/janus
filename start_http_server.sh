#!/bin/bash

# Janus HTTP Server Startup Script
# This script starts the HTTP server with proper configuration

set -e

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║         Janus HTTP Server Startup Script                      ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""

# Check if mosquitto is running (for MQTT support)
if command -v docker &> /dev/null; then
    if docker ps | grep -q janus-mosquitto; then
        echo "✓ MQTT broker (mosquitto) is running"
    else
        echo "⚠ MQTT broker not detected. Starting mosquitto..."
        docker-compose up -d mosquitto 2>/dev/null || echo "  (Docker compose not available or already running)"
    fi
else
    echo "⚠ Docker not found. MQTT functionality may not work."
fi

echo ""

# Clean up old storage if requested
if [ "$1" == "--clean" ]; then
    echo "Cleaning storage directory..."
    rm -rf data/storage/*
    echo "✓ Storage cleaned"
    echo ""
fi

# Build the server
echo "Building HTTP server..."
cargo build --release --bin http_server
echo "✓ Build complete"
echo ""

# Start the server
echo "Starting HTTP server on http://127.0.0.1:8080"
echo ""
echo "API Endpoints:"
echo "  - POST   /api/queries              (Register query)"
echo "  - POST   /api/queries/:id/start    (Start query)"
echo "  - DELETE /api/queries/:id          (Delete query)"
echo "  - GET    /api/queries              (List queries)"
echo "  - WS     /api/queries/:id/results  (Stream results)"
echo "  - POST   /api/replay/start         (Start replay)"
echo "  - POST   /api/replay/stop          (Stop replay)"
echo "  - GET    /api/replay/status        (Replay status)"
echo ""
echo "Storage:"
echo "  - Background flushing: ENABLED"
echo "  - Auto-flush every 5 seconds or when batch full"
echo ""
echo "Dashboard: Open examples/demo_dashboard.html in your browser"
echo ""
echo "═══════════════════════════════════════════════════════════════"
echo ""

# Run the server with verbose output
RUST_LOG=info ./target/release/http_server \
    --host 127.0.0.1 \
    --port 8080 \
    --storage-dir ./data/storage \
    --max-batch-size-bytes 10485760 \
    --flush-interval-ms 5000 \
    --max-total-memory-mb 1024
