#!/bin/bash
cd "$(dirname "$0")/.."
cargo run --bin http_server &
SERVER_PID=$!
sleep 3
curl -s http://localhost:8080/health | jq . || echo "Health check failed"
kill $SERVER_PID 2>/dev/null
