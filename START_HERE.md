# Janus Backend - Start Here

Use this file if you want the fastest path to a working local backend.

## Quick Start

### 1. Start the MQTT broker

```bash
docker-compose up -d mosquitto
```

### 2. Start the HTTP server

```bash
cargo run --bin http_server
```

### 3. Check health

```bash
curl http://localhost:8080/health
```

Expected response:

```json
{"message":"Janus HTTP API is running"}
```

## Optional: Open the local demo dashboard

This repository contains a small demo dashboard:

```bash
open examples/demo_dashboard.html
```

For the maintained frontend, use the separate repository:

- `https://github.com/SolidLabResearch/janus-dashboard`

## Most Useful Docs

- [GETTING_STARTED.md](./GETTING_STARTED.md)
- [docs/README_HTTP_API.md](./docs/README_HTTP_API.md)
- [docs/QUICKSTART_HTTP_API.md](./docs/QUICKSTART_HTTP_API.md)
- [docs/README.md](./docs/README.md)

## Notes

- The backend is the primary concern of this repository.
- `http_server` is the main user-facing executable.
- `stream_bus_cli` is the replay/ingestion CLI.
