# Getting Started with Janus

This guide reflects the current state of the repository.

Janus is primarily a backend Rust project. The most useful entry points today are:

- `http_server` for the HTTP/WebSocket API
- `stream_bus_cli` for replaying RDF files into storage and MQTT
- the Rust test suite for validating the engine locally

## Prerequisites

- Rust stable
- Cargo
- Git
- Docker, if you want to run the local MQTT broker

## Clone and Build

```bash
git clone https://github.com/SolidLabResearch/janus.git
cd janus

cargo build
```

## Run the Backend

### Option 1: Start the HTTP API

```bash
cargo run --bin http_server
```

The server listens on `http://127.0.0.1:8080` by default.

### Option 2: Inspect the replay CLI

```bash
cargo run --bin stream_bus_cli -- --help
```

Typical usage:

```bash
cargo run --bin stream_bus_cli -- \
  --input data/sensors.nq \
  --broker none \
  --rate 0
```

## Run Tests

```bash
cargo test --all-features
```

## Run Lint Checks

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

## Quick HTTP Flow

1. Start the server:

```bash
cargo run --bin http_server
```

2. Register a query:

```bash
curl -X POST http://localhost:8080/api/queries \
  -H "Content-Type: application/json" \
  -d '{
    "query_id": "demo_query",
    "janusql": "PREFIX ex: <http://example.org/> SELECT ?s ?p ?o FROM NAMED WINDOW ex:w ON STREAM ex:sensorStream [START 0 END 9999999999999] WHERE { WINDOW ex:w { ?s ?p ?o . } }"
  }'
```

3. Start the query:

```bash
curl -X POST http://localhost:8080/api/queries/demo_query/start
```

4. Subscribe to results:

```text
ws://localhost:8080/api/queries/demo_query/results
```

5. Stop the query when done:

```bash
curl -X POST http://localhost:8080/api/queries/demo_query/stop
```

## Project Layout

```text
janus/
├── src/
│   ├── api/           # Janus API coordination layer
│   ├── core/          # RDF event types and encoding
│   ├── execution/     # Historical execution and result conversion
│   ├── http/          # HTTP and WebSocket server
│   ├── parsing/       # JanusQL parser
│   ├── querying/      # SPARQL execution adapters
│   ├── storage/       # Segmented storage and indexing
│   ├── stream/        # Live stream processing
│   └── stream_bus/    # Replay and broker integration
├── tests/             # Integration and module tests
├── examples/          # Example clients and benchmarks
├── docs/              # Documentation
└── janus-dashboard/   # Lightweight local demo dashboard
```

## Dashboard Boundary

This repository includes a small demo dashboard under `janus-dashboard/`, but the maintained dashboard lives separately:

- `https://github.com/SolidLabResearch/janus-dashboard`

If you are working on frontend product features, use the separate dashboard repository.

## Recommended Next Reads

- [START_HERE.md](./START_HERE.md)
- [docs/README_HTTP_API.md](./docs/README_HTTP_API.md)
- [docs/STREAM_BUS_CLI.md](./docs/STREAM_BUS_CLI.md)
- [docs/README.md](./docs/README.md)
