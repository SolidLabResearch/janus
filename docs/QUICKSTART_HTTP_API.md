# Janus HTTP API Quickstart

## 1. Start the server

```bash
cargo run --bin http_server -- --host 127.0.0.1 --port 8080 --storage-dir ./data/storage
```

## 2. Check health

```bash
curl http://127.0.0.1:8080/health
```

## 3. Register a historical query

```bash
curl -X POST http://127.0.0.1:8080/api/queries \
  -H 'Content-Type: application/json' \
  -d '{
    "query_id": "historical_q1",
    "janusql": "PREFIX ex: <http://example.org/> SELECT ?s ?p ?o FROM NAMED WINDOW ex:w ON LOG ex:log [START 1700000000000 END 1700086400000] WHERE { WINDOW ex:w { ?s ?p ?o . } }"
  }'
```

## 4. Start and observe the query

```bash
curl -X POST http://127.0.0.1:8080/api/queries/historical_q1/start
curl http://127.0.0.1:8080/api/queries/historical_q1
```

Connect a WebSocket client to:

```text
ws://127.0.0.1:8080/api/queries/historical_q1/results
```

## 5. Stop and delete it

```bash
curl -X POST http://127.0.0.1:8080/api/queries/historical_q1/stop
curl -X DELETE http://127.0.0.1:8080/api/queries/historical_q1
```

For live streams, first start Mosquitto with `docker-compose up -d mosquitto`
and use the replay endpoint or [stream-bus CLI](./STREAM_BUS_CLI.md). See the
[HTTP API reference](./HTTP_API_CURRENT.md) for all endpoints.
