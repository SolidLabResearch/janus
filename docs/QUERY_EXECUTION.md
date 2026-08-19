# Query Execution

Janus routes a registered query according to its declared windows.

## Lifecycle

1. `POST /api/queries` parses and validates Janus-QL, then stores query
   metadata.
2. `POST /api/queries/:id/start` starts the applicable historical and/or live
   execution path and creates the result forwarder for WebSocket subscribers.
3. `GET /api/queries/:id/results` upgrades to a WebSocket and emits JSON result
   messages with a query id, timestamp, source, and bindings.
4. `POST /api/queries/:id/stop` stops an active query; only stopped queries can
   be deleted.

## Execution paths

- A historical fixed window is lowered to a SPARQL query over segmented
  storage and emits `Historical` results.
- A live window is lowered to the live RSP-QL path and consumes its MQTT-backed
  stream source, emitting `Live` results.
- A hybrid query creates both paths. Historical materialization is prepared for
  supported nested historical subqueries before it is made available to the
  live path.

## Status

Query metadata exposes `Registered`, `WarmingBaseline`, `Running`, `Stopped`,
or `Failed(...)`. `WarmingBaseline` is implementation compatibility behavior;
it should not be used as a public Janus-QL language feature.

See [HTTP API](./HTTP_API_CURRENT.md) for the transport contract and
[Nested Historical Subqueries](./NESTED_HISTORICAL_SUBQUERIES.md) for lowering
constraints.
