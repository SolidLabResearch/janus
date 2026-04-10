# Janus Documentation Index

This is the shortest path to understanding the current Janus implementation.

## Core Reading Order

1. [../README.md](../README.md)
2. [../GETTING_STARTED.md](../GETTING_STARTED.md)
3. [../START_HERE.md](../START_HERE.md)
4. [JANUSQL.md](./JANUSQL.md)
5. [QUERY_EXECUTION.md](./QUERY_EXECUTION.md)
6. [BASELINES.md](./BASELINES.md)
7. [HTTP_API_CURRENT.md](./HTTP_API_CURRENT.md)
8. [ANOMALY_DETECTION.md](./ANOMALY_DETECTION.md)
9. [QUICK_REFERENCE.md](./QUICK_REFERENCE.md)

## What Each File Covers

- [JANUSQL.md](./JANUSQL.md)
  - query structure
  - supported window types
  - `USING BASELINE <window> LAST|AGGREGATE`
  - how live and historical queries are derived

- [QUERY_EXECUTION.md](./QUERY_EXECUTION.md)
  - registration and parsed metadata
  - `start_query()` flow
  - historical workers
  - live workers and MQTT subscription
  - result multiplexing and runtime status

- [BASELINES.md](./BASELINES.md)
  - what baseline bootstrap does
  - `LAST` vs `AGGREGATE`
  - async warm-up behavior
  - what state is and is not retained

- [HTTP_API_CURRENT.md](./HTTP_API_CURRENT.md)
  - current REST endpoints
  - WebSocket result flow
  - request and response shapes
  - persisted query lifecycle status

- [ANOMALY_DETECTION.md](./ANOMALY_DETECTION.md)
  - when extension functions are enough
  - when baseline state helps
  - recommended query patterns

- [QUICK_REFERENCE.md](./QUICK_REFERENCE.md)
  - common local commands
  - query lifecycle endpoints
  - replay endpoints
  - smoke-test flow

## Additional Current Guides

- [STREAM_BUS_CLI.md](./STREAM_BUS_CLI.md)
- [README_HTTP_API.md](./README_HTTP_API.md)
- [QUICKSTART_HTTP_API.md](./QUICKSTART_HTTP_API.md)

## Dashboard Boundary

- Maintained dashboard repository: `https://github.com/SolidLabResearch/janus-dashboard`

## Related Code

- [../src/parsing/janusql_parser.rs](../src/parsing/janusql_parser.rs)
- [../src/api/janus_api.rs](../src/api/janus_api.rs)
- [../src/http/server.rs](../src/http/server.rs)
- [../src/stream/live_stream_processing.rs](../src/stream/live_stream_processing.rs)
- [../src/execution/historical_executor.rs](../src/execution/historical_executor.rs)
