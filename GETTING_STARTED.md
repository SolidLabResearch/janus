# Getting Started with Janus

Janus is a Rust engine for querying historical and live RDF data through one
Janus-QL model and one HTTP/WebSocket API.

## Prerequisites

- Rust stable with Cargo
- Docker and Docker Compose if you want the MQTT-backed replay flow

## Fastest Working Path

### 1. Build and test

```bash
make build
make test
```

### 2. Start the HTTP server

```bash
cargo run --bin http_server -- --host 127.0.0.1 --port 8080 --storage-dir ./data/storage
```

Verify it is up:

```bash
curl http://127.0.0.1:8080/health
```

### 3. Exercise the API

The quickest end-to-end client is the example binary:

```bash
cargo run --example http_client_example
```

That example covers query registration, start, stop, replay control, and
WebSocket result consumption.

## Optional Local Demo UI

This repository keeps a small static demo at
`examples/demo_dashboard.html` for manual browser testing.

The maintained Svelte dashboard lives in the separate
`SolidLabResearch/janus-dashboard` repository.

## Main Binaries

- `http_server`: REST and WebSocket API for query lifecycle and replay control
- `stream_bus_cli`: replay and ingestion CLI for RDF event files

## Common Commands

```bash
make build
make release
make test
make fmt
make fmt-check
make lint
make check
make ci-check
```

## Repository Layout

- `src/api`: query lifecycle orchestration
- `src/http`: REST and WebSocket server
- `src/parsing`: Janus-QL parsing
- `src/execution`: historical execution
- `src/stream`: live stream processing
- `src/storage`: segmented RDF storage
- `src/bin`: executable binaries
- `examples`: runnable examples and a minimal static demo
- `tests`: integration coverage
- `docs`: current product docs plus a small amount of retained background material

## Where to Read Next

- `README.md`
- `START_HERE.md`
- `docs/DOCUMENTATION_INDEX.md`
