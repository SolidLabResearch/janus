# Query-Defined Baseline Benchmark

This document explains how to run the Janus query-defined baseline benchmark used for the paper experiments.

The benchmark evaluates Janus-QL queries of the following form:

```sparql
DEFINE BASELINE :dayBaseline ON WINDOW :historyDay AS
SELECT ?sensor
       (AVG(?value) AS ?dayAvgValue)
WHERE {
  ?sensor :hasValue ?value .
}
GROUP BY ?sensor

REGISTER RStream :output AS
USING BASELINE :dayBaseline
SELECT ?sensor
       (AVG(?value) AS ?minuteAvgValue)
       ?dayAvgValue
       ((AVG(?value) - ?dayAvgValue) AS ?difference)
WHERE {
  WINDOW :liveMinute {
    ?sensor :hasValue ?value .
  }

  GRAPH :dayBaseline {
    ?sensor :dayAvgValue ?dayAvgValue .
  }
}
GROUP BY ?sensor ?dayAvgValue
HAVING(AVG(?value) > ?dayAvgValue)
```

The benchmark measures the cost of:

1. generating and storing historical events,
2. evaluating the historical `DEFINE BASELINE` query,
3. materializing baseline bindings into a static named graph,
4. injecting the static graph into the live RSP engine,
5. executing the live query with a `GRAPH :baselineName { ... }` join,
6. validating correctness,
7. recording CPU and memory usage.

## Benchmark modes

The benchmark supports two live replay modes:

### Accelerated mode

This is the default mode.

Historical and live events use deterministic timestamps, but live events are submitted as fast as possible. This is useful for scalability experiments because it avoids waiting in wall-clock time.

Use this mode for million-scale experiments.

### Realtime mode

In realtime mode, historical data is still generated and loaded as fast as possible, but live events are replayed with wall-clock sleeps according to the configured live rate.

This mode is intended only for short live-arrival sanity checks.

For example, with:

```text
live rate = 4 Hz
duration = 240 seconds
window size = 120 seconds
window slide = 60 seconds
```

the benchmark emits:

```text
live events = 4 × 240 = 960
expected emitted windows = 4
expected full windows = 3
warm-up windows = 1
```

The first emitted window is a warm-up window at the first slide boundary.

## Requirements

Recommended server configuration:

```text
CPU: 4 cores or more
RAM: 16 GB minimum, 32 GB preferred
Disk: local SSD preferred
OS: Linux recommended
Rust: stable toolchain
```

The 10M historical-event experiment may use several GB of memory. In previous runs, the 10M configuration peaked around 5 GB RSS.

## Build and test

From the Janus repository root:

```bash
cargo build --release
cargo test -- --nocapture
```

## Smoke test

Always run a small smoke benchmark first:

```bash
cargo run --release --bin paper_query_defined_baseline -- \
  --warmup-runs 0 \
  --runs 1 \
  --historical-events 10K \
  --baseline-entities 10
```

The output should look like:

```text
benchmark=query_defined_baseline
correctness_passed=true
warmup_runs=0 measured_runs=1
output_dir=logs/benchmark/query_defined_baseline/<timestamp>
raw_json=logs/benchmark/query_defined_baseline/<timestamp>/query_defined_baseline.raw.json
summary_csv=logs/benchmark/query_defined_baseline/<timestamp>/query_defined_baseline.summary.csv
markdown=logs/benchmark/query_defined_baseline/<timestamp>/query_defined_baseline_results.md
```

## Experiment A: historical-size scaling

This experiment fixes the baseline relation size and varies the historical log size.

It answers:

```text
How does query-defined baseline evaluation scale as the historical log grows?
```

Run:

```bash
cargo run --release --bin paper_query_defined_baseline -- \
  --warmup-runs 1 \
  --runs 3 \
  --historical-events 1M,5M,10M \
  --baseline-entities 10
```

Expected interpretation:

```text
Historical baseline evaluation should dominate runtime.
GRAPH-template materialization and static graph injection should remain small because the baseline relation has only 10 rows.
```

Important metrics:

```text
historical_events
baseline_entities
correctness_rate
baseline_eval_ms_mean/std
materialization_ms_mean/std
static_injection_ms_mean/std
first_result_overhead_ms_mean/std
peak_rss_mb_mean/std
mean_cpu_percent_mean/std
```

## Experiment B: baseline-cardinality scaling

This experiment fixes the historical log size and varies the number of baseline entities.

It answers:

```text
How do materialization and static graph injection scale as the baseline relation grows?
```

Run:

```bash
cargo run --release --bin paper_query_defined_baseline -- \
  --warmup-runs 1 \
  --runs 3 \
  --historical-events 1M \
  --baseline-entities 1,10,100,1000,10000
```

Expected interpretation:

```text
Historical baseline evaluation should stay in roughly the same range because the historical log size is fixed.
Materialization and static graph injection should increase with baseline cardinality.
```

Important metrics:

```text
baseline_entities
baseline_binding_count_mean
materialized_quad_count_mean
baseline_eval_ms_mean/std
materialization_ms_mean/std
static_injection_ms_mean/std
peak_rss_mb_mean/std
mean_cpu_percent_mean/std
```

## Experiment C: realtime live replay sanity check

This experiment validates live behavior under wall-clock pacing.

It answers:

```text
Does the baseline variant preserve live-window behavior when events arrive in realtime?
```

Run:

```bash
cargo run --release --bin paper_query_defined_baseline -- \
  --warmup-runs 0 \
  --runs 1 \
  --historical-events 10000 \
  --baseline-entities 10 \
  --live-replay-mode realtime \
  --live-rate-hz 4 \
  --live-duration-seconds 240 \
  --live-window-size-seconds 120 \
  --live-window-slide-seconds 60
```

Expected behavior:

```text
live_event_count = 960
expected_emitted_windows = 4
expected_full_windows = 3
warmup_window_count = 1
observed_baseline_rows = 40
observed_live_only_rows = 40
correctness_passed = true
```

The first result should appear around the first slide boundary, approximately 60 seconds.

## CLI options

### Historical event counts

The benchmark accepts raw integers:

```bash
--historical-events 1000000,5000000,10000000
```

It also accepts uppercase shorthand:

```bash
--historical-events 1K,10K,1M,5M,10M
```

Lowercase `m` is not used for million, to avoid confusion with minutes.

### Baseline entity counts

```bash
--baseline-entities 1,10,100,1000,10000
```

### Runs and warmups

```bash
--warmup-runs 1
--runs 3
```

Warmup runs are excluded from the summary statistics.

### Live replay mode

```bash
--live-replay-mode accelerated
--live-replay-mode realtime
```

Default:

```text
accelerated
```

### Realtime live options

```bash
--live-rate-hz 4
--live-duration-seconds 240
--live-window-size-seconds 120
--live-window-slide-seconds 60
```

### Verbose output

```bash
--verbose
```

Verbose mode prints per-run details and internal progress logs.

### Debug lowered query

```bash
--debug-lowered-query
```

This prints the lowered live query with line numbers. It is useful for debugging parser/lowering failures.

## Output artifacts

Each benchmark run writes to:

```text
logs/benchmark/query_defined_baseline/<timestamp>/
```

The main files are:

```text
query_defined_baseline.raw.json
query_defined_baseline.summary.csv
query_defined_baseline_results.md
```

### Raw JSON

Contains per-run measurements and correctness diagnostics.

Useful for debugging and detailed inspection.

### Summary CSV

Contains one row per configuration with mean/std metrics.

Useful for plotting and paper table generation.

### Markdown summary

Contains a compact paper-readable table.

Useful for quickly copying results into notes or drafts.

## Resource metrics

The benchmark samples whole-process CPU and memory usage.

Recorded metrics include:

```text
peak_rss_mb
mean_rss_mb
peak_cpu_percent
mean_cpu_percent
sample_count
```

These are whole-run process measurements. They include historical generation, storage writes, baseline evaluation, materialization, static injection, and live replay. They are not phase-isolated.

CPU percentages above 100% can occur if multiple cores/threads are active.

## Recommended server workflow

Run benchmarks inside `tmux` so the process survives SSH disconnects:

```bash
tmux new -s janus-bench
```

Inside the session:

```bash
cargo build --release
cargo test -- --nocapture

cargo run --release --bin paper_query_defined_baseline -- \
  --warmup-runs 0 \
  --runs 1 \
  --historical-events 10K \
  --baseline-entities 10

cargo run --release --bin paper_query_defined_baseline -- \
  --warmup-runs 1 \
  --runs 3 \
  --historical-events 1M,5M,10M \
  --baseline-entities 10

cargo run --release --bin paper_query_defined_baseline -- \
  --warmup-runs 1 \
  --runs 3 \
  --historical-events 1M \
  --baseline-entities 1,10,100,1000,10000

cargo run --release --bin paper_query_defined_baseline -- \
  --warmup-runs 0 \
  --runs 1 \
  --historical-events 10000 \
  --baseline-entities 10 \
  --live-replay-mode realtime \
  --live-rate-hz 4 \
  --live-duration-seconds 240 \
  --live-window-size-seconds 120 \
  --live-window-slide-seconds 60
```

Detach from `tmux` with:

```text
Ctrl+b, then d
```

Reattach with:

```bash
tmux attach -t janus-bench
```

## Archiving results

After running experiments, archive the benchmark outputs:

```bash
tar -czf janus_query_defined_baseline_results_$(date +%Y%m%d_%H%M%S).tar.gz \
  logs/benchmark/query_defined_baseline
```

Also save the server metadata:

```bash
{
  echo "date=$(date -Iseconds)"
  echo "host=$(hostname)"
  echo "commit=$(git rev-parse HEAD)"
  echo "rustc=$(rustc --version)"
  echo "cargo=$(cargo --version)"
  echo
  uname -a
  echo
  lscpu
  echo
  free -h
  echo
  df -h .
} > logs/benchmark/server_metadata_$(date +%Y%m%d_%H%M%S).txt
```

## Paper interpretation

The benchmark is intended to support the following claims:

```text
1. Query-defined baselines are evaluated over the historical log before live processing starts.

2. The resulting baseline bindings are materialized into a static named graph using the explicit GRAPH template in the registered query.

3. The live query can then use this static baseline graph together with live stream windows.

4. Historical baseline evaluation cost grows with historical log size.

5. GRAPH-template materialization and static graph injection scale with the number of baseline rows/quads.

6. For small baseline relations, materialization and injection are negligible compared with historical evaluation.

7. Realtime replay confirms that the query-defined baseline variant preserves live-window emission behavior under wall-clock event arrival.
```

Important caveat:

```text
The benchmark currently reports whole-process RSS and CPU usage. Memory usage can grow substantially with historical event count and baseline cardinality, so memory results should be reported explicitly rather than hidden.
```
