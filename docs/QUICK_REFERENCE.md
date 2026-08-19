# Janus Quick Reference

## Common commands

```bash
make build
make test
make check
cargo run --bin http_server -- --help
cargo run --bin stream_bus_cli -- --help
cargo bench --no-run
```

## HTTP routes

```text
GET    /health
GET    /ops/status
POST   /api/queries
GET    /api/queries
GET    /api/queries/:id
POST   /api/queries/:id/start
POST   /api/queries/:id/stop
DELETE /api/queries/:id
GET    /api/queries/:id/results     WebSocket upgrade
POST   /api/replay/start
POST   /api/replay/stop
GET    /api/replay/status
```

## Query window forms

```sparql
ON LOG ex:log [START 1700000000000 END 1700086400000]
ON LOG ex:log [OFFSET 86400000 RANGE 3600000 STEP 30000]
ON STREAM ex:stream [RANGE 60000 STEP 30000]
```

Use [Janus-QL](./JANUSQL.md) for supported syntax and
[HTTP API](./HTTP_API_CURRENT.md) for request and response shapes.
