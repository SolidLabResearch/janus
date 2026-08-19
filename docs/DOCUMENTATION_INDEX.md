# Janus Documentation Index

This index separates current operational documentation from historical design
records and result templates. Start with the current guides; dated decisions
are retained for project history, not as statements of present behavior.

## Start here

1. [Repository README](../README.md) — scope, prerequisites, and primary commands.
2. [Getting Started](../GETTING_STARTED.md) — shortest local path.
3. [Janus-QL](./JANUSQL.md) — the public query surface and validation rules.
4. [HTTP API](./HTTP_API_CURRENT.md) — current REST and WebSocket endpoints.
5. [Query Execution](./QUERY_EXECUTION.md) — registration and runtime paths.
6. [Live Streaming Guide](./LIVE_STREAMING_GUIDE.md) — MQTT and replay setup.

## Current guides

| Topic | Canonical document |
| --- | --- |
| Query language | [JANUSQL.md](./JANUSQL.md) |
| Window semantics | [WINDOW_TYPES_EXPLAINED.md](./WINDOW_TYPES_EXPLAINED.md) |
| Query lifecycle and execution | [QUERY_EXECUTION.md](./QUERY_EXECUTION.md) |
| REST and WebSocket API | [HTTP_API_CURRENT.md](./HTTP_API_CURRENT.md) |
| HTTP quick start | [QUICKSTART_HTTP_API.md](./QUICKSTART_HTTP_API.md) |
| Live MQTT and replay | [LIVE_STREAMING_GUIDE.md](./LIVE_STREAMING_GUIDE.md) |
| Replay CLI | [STREAM_BUS_CLI.md](./STREAM_BUS_CLI.md) |
| Historical materialization | [NESTED_HISTORICAL_SUBQUERIES.md](./NESTED_HISTORICAL_SUBQUERIES.md) |
| Baseline compatibility | [BASELINES.md](./BASELINES.md) |
| Anomaly-query guidance | [ANOMALY_DETECTION.md](./ANOMALY_DETECTION.md) |
| Backend benchmarks | [BENCHMARKING.md](./BENCHMARKING.md) |
| Paper benchmark harnesses | [PAPER_BENCHMARKING.md](./PAPER_BENCHMARKING.md) |
| Benchmark implementation | [citybench_congestion_hybrid_scaling.md](./benchmarks/citybench_congestion_hybrid_scaling.md) |

## Reference and historical material

- [BENCHMARK_RESULTS.md](./BENCHMARK_RESULTS.md) and
  [JANUS_BENCHMARK_RESULTS_SUMMARY.md](./JANUS_BENCHMARK_RESULTS_SUMMARY.md)
  contain retained historical summaries. They are not current performance
  guarantees.
- [PAPER_BENCHMARK_RESULTS_TEMPLATE.md](./PAPER_BENCHMARK_RESULTS_TEMPLATE.md)
  is a reporting template, not a results claim.
- [docs/decisions/](./decisions/) contains dated architecture and benchmark
  decisions. Where a decision conflicts with executable code or a current
  guide, the current guide and code take precedence.

## Compatibility aliases

[HTTP_API.md](./HTTP_API.md) and [README_HTTP_API.md](./README_HTTP_API.md)
are retained as compatibility entry points and direct readers to the current
HTTP API reference. [README.md](./README.md) is the short in-directory index.
