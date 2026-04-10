# Janus HTTP API Quickstart

This is the shortest backend validation flow.

## 1. Start the server

```bash
cargo run --bin http_server
```

## 2. Check health

```bash
curl http://localhost:8080/health
```

## 3. Register a query

```bash
curl -X POST http://localhost:8080/api/queries \
  -H "Content-Type: application/json" \
  -d '{
    "query_id": "test_query",
    "janusql": "PREFIX ex: <http://example.org/> SELECT ?s ?p ?o FROM NAMED WINDOW ex:w ON STREAM ex:sensorStream [START 0 END 9999999999999] WHERE { WINDOW ex:w { ?s ?p ?o . } }"
  }'
```

## 4. Start the query

```bash
curl -X POST http://localhost:8080/api/queries/test_query/start
```

## 5. Connect to the results WebSocket

```text
ws://localhost:8080/api/queries/test_query/results
```

## 6. Stop and delete the query

```bash
curl -X POST http://localhost:8080/api/queries/test_query/stop
curl -X DELETE http://localhost:8080/api/queries/test_query
```

## Replay Example

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

Check replay metrics:

```bash
curl http://localhost:8080/api/replay/status
```

## Optional Demo Client

This repository still contains a static demo HTML client for manual API
testing:

```bash
open examples/demo_dashboard.html
```

For the maintained frontend, use:

- `https://github.com/SolidLabResearch/janus-dashboard`
