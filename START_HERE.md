# Janus Start Here

## Quick Start

```bash
docker-compose up -d mosquitto
cargo run --bin http_server -- --host 127.0.0.1 --port 8080 --storage-dir ./data/storage
curl http://127.0.0.1:8080/health
```

In another terminal, run:

```bash
cargo run --example http_client_example
```

## What To Use

- `http_server` is the main backend entry point
- `stream_bus_cli` is the ingestion and replay CLI
- `examples/demo_dashboard.html` is an optional minimal manual demo
- the maintained Svelte dashboard lives in the separate `janus-dashboard` repository

## Current Docs

- `README.md`
- `GETTING_STARTED.md`
- `docs/DOCUMENTATION_INDEX.md`
- `docs/HTTP_API_CURRENT.md`
- `docs/README_HTTP_API.md`
- `docs/QUICKSTART_HTTP_API.md`
- `docs/QUICK_REFERENCE.md`
