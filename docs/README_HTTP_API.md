# Janus HTTP API Guide

This guide documents the current backend HTTP/WebSocket surface in this repository.

## Overview

The HTTP server exposes:

- REST endpoints for registering, starting, stopping, listing, and deleting queries
- a WebSocket endpoint for streaming query results
- replay control endpoints for ingesting RDF files into storage and optional MQTT publication

Run it with:

```bash
cargo run --bin http_server
```

Default address:

- `http://127.0.0.1:8080`

## Query Lifecycle

### 1. Register

```bash
curl -X POST http://localhost:8080/api/queries \
  -H "Content-Type: application/json" \
  -d '{
    "query_id": "sensor_analysis",
    "janusql": "PREFIX ex: <http://example.org/> SELECT ?sensor ?temp FROM NAMED WINDOW ex:w ON STREAM ex:sensorStream [START 0 END 9999999999999] WHERE { WINDOW ex:w { ?sensor ex:temperature ?temp . } }"
  }'
```

### 2. Start

```bash
curl -X POST http://localhost:8080/api/queries/sensor_analysis/start
```

### 3. Subscribe to results

```text
ws://localhost:8080/api/queries/sensor_analysis/results
```

The server sends JSON messages with:

- `query_id`
- `timestamp`
- `type`
- `source` as `historical` or `live`
- `bindings`

### 4. Stop

```bash
curl -X POST http://localhost:8080/api/queries/sensor_analysis/stop
```

### 5. Delete

Deletion is only allowed after the query is stopped.

```bash
curl -X DELETE http://localhost:8080/api/queries/sensor_analysis
```

## Replay Endpoints

### Start replay

```bash
curl -X POST http://localhost:8080/api/replay/start \
  -H "Content-Type: application/json" \
  -d '{
    "input_file": "data/sensors.nq",
    "broker_type": "none",
    "topics": ["sensors"],
    "rate_of_publishing": 1000,
    "loop_file": false,
    "add_timestamps": true
  }'
```

### Stop replay

```bash
curl -X POST http://localhost:8080/api/replay/stop
```

### Replay status

```bash
curl http://localhost:8080/api/replay/status
```

## Endpoint Summary

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/health` | Health check |
| POST | `/api/queries` | Register query |
| GET | `/api/queries` | List queries |
| GET | `/api/queries/:id` | Query details |
| POST | `/api/queries/:id/start` | Start query |
| POST | `/api/queries/:id/stop` | Stop query |
| DELETE | `/api/queries/:id` | Delete stopped query |
| WS | `/api/queries/:id/results` | Stream results |
| POST | `/api/replay/start` | Start replay |
| POST | `/api/replay/stop` | Stop replay |
| GET | `/api/replay/status` | Replay status |

## Local Demo Dashboard

You can still use the demo HTML client included in this repository:

```bash
open examples/demo_dashboard.html
```

The maintained dashboard lives separately:

- `https://github.com/SolidLabResearch/janus-dashboard`

## Related Docs

- [QUICKSTART_HTTP_API.md](QUICKSTART_HTTP_API.md)
- [HTTP_API.md](HTTP_API.md)
- [STREAM_BUS_CLI.md](STREAM_BUS_CLI.md)
