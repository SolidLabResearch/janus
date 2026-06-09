# Benchmarking

This document defines the current benchmark harness for Janus backend work.

## Goals

The benchmark suite is split by engine path so regressions are easier to localize:

- `storage_write`: ingest-only storage throughput
- `historical_fixed`: fixed historical window execution
- `historical_sliding`: sliding historical window execution
- `live_injection`: live event ingestion to first emitted result
- `hybrid_baseline`: live window plus static baseline join
- `janusql_e2e`: HTTP/WebSocket Janus-QL first-result latency for historical queries
- `janusql_live_mqtt_e2e`: HTTP/WebSocket Janus-QL first-result latency for broker-backed live and hybrid queries

These benchmarks are intended for backend performance work on the current repository state.
They are not a substitute for a public performance report.

## Run Commands

`cargo bench` already uses Cargo's optimized `bench` profile, so there is no extra `--release`
flag to pass here:

```bash
cargo bench --bench storage_write
cargo bench --bench historical_fixed
cargo bench --bench historical_sliding
cargo bench --bench live_injection
cargo bench --bench hybrid_baseline
cargo bench --bench janusql_e2e
cargo bench --bench janusql_live_mqtt_e2e
```

To verify that all benchmark targets compile without running them:

```bash
cargo bench --no-run
```

## What Each Benchmark Measures

### `storage_write`

Measures `StreamingSegmentedStorage::write_rdf_event` throughput on fresh storage with large flush
thresholds so the benchmark isolates write-path cost rather than background flush cost.

### `historical_fixed`

Measures a single fixed historical window query over deterministic stored events in `ex:graph1`.

### `historical_sliding`

Measures a sliding historical window scan over deterministic stored events placed in a recent time
range so the windows stay valid relative to the executor's wall-clock based offset logic.

### `live_injection`

Measures the live path from event injection into `LiveStreamProcessing` to the first emitted
result. The harness injects a sentinel event at the end to close remaining windows deterministically.

### `hybrid_baseline`

Measures a hybrid-style query where live window bindings join against static baseline data already
materialized into the live engine. This isolates hybrid join cost without requiring an MQTT broker
or the HTTP server.

### `janusql_e2e`

Measures end-to-end Janus-QL latency through the HTTP API:

- `historical_preloaded`: register query, start query, subscribe over WebSocket, wait for first result
- `historical_via_replay`: start replay, register query, start query, subscribe over WebSocket, wait for first result

This target currently covers historical Janus-QL end-to-end flows. Full live or hybrid HTTP e2e
benchmarks are covered separately in `janusql_live_mqtt_e2e`, because Janus live execution is
broker-driven at the API layer.

### `janusql_live_mqtt_e2e`

Measures broker-backed Janus-QL latency through the HTTP API and WebSocket result stream:

- `mqtt_live_first_result`: register a live query, start it, subscribe over WebSocket, publish MQTT events, wait for the first live result
- `mqtt_hybrid_first_result`: register a hybrid query with historical baseline bootstrap, start it, wait for baseline warm-up, subscribe over WebSocket, publish MQTT events, wait for the first live result

This target requires an MQTT broker reachable from the benchmark process. By default it uses
`127.0.0.1:1883` and topic `sensors`, and it can be overridden with:

- `JANUS_BENCH_MQTT_HOST`
- `JANUS_BENCH_MQTT_PORT`
- `JANUS_BENCH_MQTT_TOPIC`
- `JANUS_BENCH_MQTT_STREAM_URI`

## Reproducibility Rules

When comparing numbers across commits:

1. Run on a quiet machine.
2. Use the same hardware, Rust toolchain, and bench profile.
3. Avoid running multiple benchmark targets in parallel.
4. Capture the exact command, branch, and commit SHA with the results.
5. Record CPU model, RAM, OS version, and whether the system was thermally constrained.

## Recommended Workflow

For routine optimization work:

```bash
cargo bench --no-run
cargo bench --bench storage_write
cargo bench --bench historical_fixed
cargo bench --bench historical_sliding
cargo bench --bench live_injection
cargo bench --bench hybrid_baseline
cargo bench --bench janusql_e2e
cargo bench --bench janusql_live_mqtt_e2e
```

For benchmark result reviews, attach the Criterion output directory from `target/criterion/`.
