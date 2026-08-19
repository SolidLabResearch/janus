# HTTP API

This document describes the current Janus HTTP and WebSocket API.

The server binary is:

```bash
cargo run --bin http_server -- --host 127.0.0.1 --port 8080 --storage-dir ./data/storage
```

## Endpoints

### Health

`GET /health`

Returns a service health payload with storage background-flush status.

Healthy response:

```json
{
  "status": "ok",
  "message": "Janus HTTP API is running",
  "storage_status": "ok",
  "storage_error": null
}
```

If background storage flushing has failed, the endpoint returns HTTP `503
Service Unavailable` with:

```json
{
  "status": "degraded",
  "message": "Janus HTTP API is running with storage errors",
  "storage_status": "error",
  "storage_error": "Background flush failed: ..."
}
```

### Ops Status

`GET /ops/status`

Returns a richer operational snapshot with:

- overall service status
- storage background-flush health
- replay metrics
- query lifecycle counts

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

### Start Replay

`POST /api/replay/start`

Starts replay from an N-Triples or N-Quads input file. The request accepts:

```json
{
  "input_file": "data/sensors.nq",
  "broker_type": "mqtt",
  "topics": ["sensors"],
  "rate_of_publishing": 64,
  "loop_file": false,
  "add_timestamps": true,
  "mqtt_config": {
    "host": "localhost",
    "port": 1883,
    "client_id": "janus-replay",
    "keep_alive_secs": 30
  }
}
```

`broker_type` is `mqtt` or `none`. If omitted, it defaults to `none`; topics,
rate, and timestamp insertion also have server defaults. Only one replay can
run at a time.

### Stop Replay

`POST /api/replay/stop`

Stops the active replay. It returns a bad request when no replay is running.

### Replay Status

`GET /api/replay/status`

Returns whether replay is running plus read, published, stored, and error
counts, events per second, and elapsed seconds.

## Typical Flow

1. `POST /api/queries`
2. `POST /api/queries/:id/start`
3. Connect `WS /api/queries/:id/results`
4. Read query results
5. `POST /api/queries/:id/stop`
6. `DELETE /api/queries/:id`

## Baseline compatibility

The implementation retains baseline-oriented compatibility behavior. A query
using that internal path may enter `WarmingBaseline` after start, and
baseline-dependent joins can produce results only after warm-up completes.
This is not a public Janus-QL language guarantee; see
[BASELINES.md](./BASELINES.md) for the boundary.
