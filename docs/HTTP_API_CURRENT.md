# HTTP API

This document describes the current Janus HTTP and WebSocket API.

The server binary is:

```bash
cargo run --bin http_server -- --host 127.0.0.1 --port 8080 --storage-dir ./data/storage
```

## Endpoints

### Health

`GET /health`

Returns a simple success payload.

### Register Query

`POST /api/queries`

Request body:

```json
{
  "query_id": "anomaly_q1",
  "janusql": "PREFIX ex: <http://example.org/> ...",
  "baseline_mode": "aggregate"
}
```

`baseline_mode` is optional and accepts:

- `aggregate`
- `last`

If the Janus-QL query contains `USING BASELINE ...`, that query-level clause overrides this registration default at execution time.

### List Queries

`GET /api/queries`

Response shape:

```json
{
  "queries": ["q1", "q2"],
  "total": 2
}
```

### Get Query Details

`GET /api/queries/:id`

Response includes:

- `query_id`
- `query_text`
- `baseline_mode`
- `registered_at`
- `execution_count`
- `is_running`
- `status`

Possible `status` values include:

- `Registered`
- `WarmingBaseline`
- `Running`
- `Stopped`
- `Failed(...)`

### Start Query

`POST /api/queries/:id/start`

Starts execution and creates the internal forwarder used for WebSocket subscribers.

### Stop Query

`POST /api/queries/:id/stop`

Stops a running query.

### Delete Query

`DELETE /api/queries/:id`

Deletes a stopped query from the registry.

### Stream Results

`WS /api/queries/:id/results`

WebSocket messages are JSON-encoded query results containing:

- `query_id`
- `timestamp`
- `source`
- `bindings`

`source` is either:

- `Historical`
- `Live`

## Typical Flow

1. `POST /api/queries`
2. `POST /api/queries/:id/start`
3. Connect `WS /api/queries/:id/results`
4. Read query results
5. `POST /api/queries/:id/stop`
6. `DELETE /api/queries/:id`

## Notes on Baseline Queries

For baseline-backed hybrid queries:

- the query may enter `WarmingBaseline` after start
- live execution still starts immediately
- results that depend on baseline joins may appear only after warm-up finishes
