# Janus Documentation Index

This is the shortest path to understanding the current Janus implementation.

## Core Reading Order

1. [../README.md](../README.md)
2. [JANUSQL.md](./JANUSQL.md)
3. [QUERY_EXECUTION.md](./QUERY_EXECUTION.md)
4. [BASELINES.md](./BASELINES.md)
5. [HTTP_API_CURRENT.md](./HTTP_API_CURRENT.md)
6. [ANOMALY_DETECTION.md](./ANOMALY_DETECTION.md)

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
  - `baseline_mode` registration fallback

- [ANOMALY_DETECTION.md](./ANOMALY_DETECTION.md)
  - when extension functions are enough
  - when baseline state helps
  - recommended query patterns

## Legacy Material

The following files remain useful as background, but they are not the main entrypoint for the current code:

- [ARCHITECTURE.md](./ARCHITECTURE.md)
- [EXECUTION_ARCHITECTURE.md](./EXECUTION_ARCHITECTURE.md)
- [HTTP_API.md](./HTTP_API.md)
- [README_HTTP_API.md](./README_HTTP_API.md)
- [SETUP_GUIDE.md](./SETUP_GUIDE.md)

## Related Code

- [../src/parsing/janusql_parser.rs](../src/parsing/janusql_parser.rs)
- [../src/api/janus_api.rs](../src/api/janus_api.rs)
- [../src/http/server.rs](../src/http/server.rs)
- [../src/stream/live_stream_processing.rs](../src/stream/live_stream_processing.rs)
- [../src/execution/historical_executor.rs](../src/execution/historical_executor.rs)
