# Paper Benchmarking

This document defines the paper-facing benchmark harnesses for the two current Janus hypotheses:

- H1.1: unified Janus hybrid execution versus a decomposed Oxigraph-based baseline
- H2: historical and hybrid query latency scaling with dataset size

The comparison is intentionally limited to:

1. `janus_unified`
2. `decomposed_oxigraph`

No Apache Jena or SPARQL-ST path is included in this harness version.

## Purpose

These harnesses are not Criterion microbenchmarks. They are paper-oriented runners that emit:

- raw JSONL with one row per recorded run
- summary CSV designed for paper tables
- a scaling-fit CSV for H2
- reproducibility metadata on every raw row

Default output roots:

- `target/paper_benchmarks/paper_hybrid_coordination`
- `target/paper_benchmarks/paper_historical_scaling`

## Warmup Behavior

Both binaries support:

- `--warmup-runs N`
- `--include-warmups`

Warmup runs execute the same workload as measured runs.

Default behavior:

- warmups are executed
- warmups are excluded from the emitted JSONL and summary CSV files

If `--include-warmups` is set, warmup rows are written with `is_warmup: true`.

## Cold Vs Warm Mode

Both binaries support:

- `--mode cold|warm`

Default mode:

- `warm`

### H1.1 `paper_hybrid_coordination`

`warm` mode:

- reuses a prebuilt historical workload in memory
- reuses pre-generated live events
- still initializes a fresh live engine per run
- measures query execution, baseline retrieval/materialization, live publication, and first-result latency

`cold` mode:

- rebuilds historical storage and workload data per run
- rebuilds live event inputs per run
- includes workload/store setup cost in end-to-end latency

### H2 `paper_historical_scaling`

`warm` mode:

- reuses a pre-generated historical dataset per dataset size
- measures query execution path on an already initialized dataset

`cold` mode:

- regenerates and reloads the dataset per run
- includes dataset/store setup cost in latency

## Measured Vs Estimated Metrics

The harness distinguishes direct timing/size observations from logical or derived values.

### H1.1 Raw Metrics

Directly recorded timestamps:

- `client_start`
- `query_registered`
- `historical_start`
- `historical_done`
- `live_ready`
- `first_event_published`
- `first_result_engine`
- `first_result_client`

Directly derived from timestamps:

- `e2e_latency_ms = first_result_client - client_start`

Estimated fields:

- `estimated_useful_engine_work_ms`
  - Janus unified: `(historical_done - historical_start) + (first_result_engine - first_event_published)`
  - decomposed Oxigraph: same formula, where the historical term is external Oxigraph historical work and the live term is Janus live processing until the joined result is available
- `estimated_coordination_overhead_ms = e2e_latency_ms - estimated_useful_engine_work_ms`
- `estimated_external_transfer_bytes`
  - logical serialized size of intermediate payloads exchanged between decomposed components

Measured byte payload sizes:

- `historical_intermediate_bytes`
- `live_intermediate_bytes`
- `final_result_bytes`

Structural counts:

- `components`
- `process_boundaries`
- `serialization_steps`

Result correctness:

- `result_count`
- `result_hash`
- `equivalent_to_baseline`

### H2 Raw Metrics

Directly measured or observed:

- `latency_ms`
- `result_count`
- `peak_rss_mb` when available

Logical or derived:

- `logical_quads_scanned`
  - number of quads logically covered by the requested time range, not a low-level physical storage scan count
- `selectivity = result_count / dataset_size_quads`
- `throughput_quads_per_sec = logical_quads_scanned / latency`

Result correctness:

- `result_hash`

## H1.1 Hybrid Query Semantics

The H1.1 benchmark validates unified versus decomposed hybrid execution. The query model integrates three distinct elements:
- Historical component: Executes a query over historical events stored in a segmented storage backend (using a SPARQL engine to retrieve baseline bindings).
- Live stream/window component: Processes real-time events through a sliding window stream operator ([RANGE 10000 STEP 1000]).
- Hybrid result: Merges the live stream window output with the materialized historical baseline statements matching on the join key (sensor).

### Execution Architectures

#### Janus-QL Unified Execution
Janus-QL compiles the historical and live stream elements into a single execution graph. The historical sub-query is executed once, and its results are materialized as internal static bindings. As live events stream into the engine, they are joined inline with the static bindings within the same process. No serialization or IPC boundaries are crossed to merge live stream and historical data.

#### Decomposed Baseline
The decomposed baseline, labeled "Oxigraph historical + Janus live window processor + external join", coordinates execution across separate stages:
1. Oxigraph historical query: An external Oxigraph engine queries the historical dataset to retrieve the baseline bindings. These are returned and materialized into memory in the client coordinate space.
2. Janus live window processor: A separate instance of the Janus live processing engine executes a live-only RSP-QL query over the live events stream.
3. External join/materialization: The client coordinate space collects the intermediate live window results and performs an external hash join against the materialized historical bindings on the matching sensor variable.

### Equivalence Target
The primary target for correctness equivalence checking is the final hybrid result (hybrid_result_hash and hybrid_result_count). Historical equivalence is monitored as diagnostic evidence, whereas live-only intermediate equivalence is not directly applicable during benchmark execution because Janus Unified does not produce a separate intermediate live result.

## Result Equivalence Checking

Each run emits `result_hash`, computed as a deterministic SHA-256 hash over canonicalized result rows:

- row keys sorted lexicographically
- normalized string values
- canonical rows serialized before hashing

For H1.1:

- `janus_unified` is compared against `decomposed_oxigraph` for the same run workload
- `equivalent_to_baseline` is set on the Janus row
- the decomposed baseline row uses `equivalent_to_baseline: null`

For H2:

- no baseline equivalence field is emitted because H2 is a scaling study across query classes, not a paired-system comparison

## H1.1 Command

```bash
cargo run --release --bin paper_hybrid_coordination -- \
  --warmup-runs 1 \
  --runs 10 \
  --historical-events 10000 \
  --live-events 32 \
  --mode warm
```

Outputs:

- `paper_hybrid_coordination.raw.jsonl`
- `paper_hybrid_coordination.summary.csv`

## H1.2 Sustained Hybrid Window Execution

The H1.2 benchmark evaluates sustained hybrid query execution performance over repeated sliding windows. It supports both deterministic virtual event-time replay and slower wall-clock replay.

While H1.1 focuses on the latency of registering and obtaining the very first hybrid query result (coordination overhead check), H1.2 models a continuous stream processor running over a logical stream duration (by default 180 or 240 logical seconds).

### Execution Semantics

- Virtual Event-Time Ingestion (`--time-mode virtual`, default): Events are published back-to-back in virtual time rather than sleeping in wall-clock time. The live stream engine's sliding window boundaries are advanced entirely by the event timestamps. This is the deterministic, high-repeat mode and is the correct default for engine-overhead measurement.
- Wall-Clock Replay (`--time-mode wall-clock`): Events are published according to real elapsed time. `event_rate_hz` determines the target spacing between events, so `event_rate_hz=4` means one event every 250 ms. Event timestamps still follow the logical schedule; only the publication timing changes. This mode is intended as a realism and sanity check for user-observed result timing, not as the main repeated performance matrix.
- Watermark/Flush Behavior: To reliably finalize and evaluate the final window, a watermark sentinel event is published at the end of the stream (offset by +20 seconds past the logical duration bounds), forcing all remaining active windows to close and emit their results.
- Wall-Clock Sentinel Timing: In wall-clock mode, the harness schedules normal events against a monotonic `Instant` and sends the sentinel only after the logical live duration has elapsed. This keeps replay duration approximately aligned with `logical_live_duration_seconds * 1000`, plus processing and synchronization overhead.
- Decomposed Baseline coordination: For each completed window, the decomposed baseline performs:
  1. Oxigraph historical retrieval (once).
  2. Janus live window processor execution (returns a live stream result for each slide).
  3. Client coordinate join (combining the window's live result and the materialized historical bindings).
  These stages are measured slide-by-slide to determine the per-window hybrid latency.

### Latency Interpretation

- Processing latency: CPU/runtime cost after data and window readiness. In H1.2 this remains `first_hybrid_result_latency_ms` and the per-window hybrid latency fields.
- Wall-clock result offset: When a user would actually observe a result since replay start. In wall-clock mode this is reported via `first_hybrid_result_wall_clock_ms` and per-window `window_result_wall_clock_offsets_ms`, with paper-facing summaries using the in-horizon p50/p95 offset fields.

Virtual mode is still the recommended mode for final repeated measurements because it is fast, deterministic, and isolates engine/runtime overhead. Wall-clock mode is slower and more sensitive to OS scheduling jitter, so use it as a realism/sanity benchmark.

### Window Horizon Filtering and Sentinel Flushes

For logical_live_duration_seconds=240, window_size_seconds=120, and window_slide_seconds=60:
- The virtual event-time stream starts at 1900000000000 ms (relative 0 seconds) and ends at 1900000239000 ms (relative 239 seconds).
- The first completed window triggers at 59 seconds offset (ending at 179 seconds), and the second triggers at 119 seconds offset (ending at 239 seconds). Both end before or at the 240 seconds horizon, meaning they contain only normal stream events.
- The third window (ending at 299 seconds) and fourth window (ending at 359 seconds) extend past the 240 seconds logical duration. These are only completed and emitted because the close_stream call publishes a sentinel watermark at 259 seconds, forcing a flush of all active windows.
- To prevent sentinel-finalized flush windows or other out-of-horizon windows from skewing summary statistics, the harness separates completed windows:
  - completed_windows_total: The total count of all windows emitted (4).
  - completed_windows_in_horizon: The count of windows ending within the logical duration (2).
  - flush_windows: The count of windows finalized only by the sentinel flush (2).
- Latency, equivalence, result count, and transfer byte statistics are computed exclusively over completed_windows_in_horizon.

## H1.2 Command

```bash
cargo run --release --bin paper_sustained_hybrid -- \
  --warmup-runs 1 \
  --runs 5 \
  --historical-events 10000 \
  --live-duration-seconds 240 \
  --event-rate-hz 1 \
  --window-size-seconds 120 \
  --window-slide-seconds 60 \
  --mode warm \
  --time-mode virtual
```

Outputs:

- `paper_sustained_hybrid.raw.jsonl`
- `paper_sustained_hybrid.summary.csv`

Recommended wall-clock sanity check:

```bash
cargo run --release --bin paper_sustained_hybrid -- \
  --warmup-runs 0 \
  --runs 1 \
  --historical-events 1000 \
  --live-duration-seconds 20 \
  --event-rate-hz 4 \
  --window-size-seconds 10 \
  --window-slide-seconds 5 \
  --mode warm \
  --time-mode wall-clock \
  --debug-equivalence \
  --output-dir target/paper_benchmarks/pre_final_h1_2_wall_clock_short
```

Optional longer paper sanity check:

```bash
cargo run --release --bin paper_sustained_hybrid -- \
  --warmup-runs 0 \
  --runs 1 \
  --historical-events 10000 \
  --live-duration-seconds 180 \
  --event-rate-hz 4 \
  --window-size-seconds 120 \
  --window-slide-seconds 60 \
  --mode warm \
  --time-mode wall-clock \
  --debug-equivalence \
  --output-dir target/paper_benchmarks/h1_2_wall_clock_180s_4hz
```

## H2 Command

```bash
cargo run --release --bin paper_historical_scaling -- \
  --warmup-runs 1 \
  --runs 5 \
  --dataset-sizes 100000,500000,1000000,5000000 \
  --mode warm
```

Outputs:

- `paper_historical_scaling.raw.jsonl`
- `paper_historical_scaling.summary.csv`
- `paper_historical_scaling.fit.csv`

## H2 Scaling Fit

For each `query_type`, the harness computes a simple linear model:

```text
latency_ms = intercept + slope * dataset_size_quads
```

Reported columns:

- `query_type`
- `mode`
- `slope_ms_per_100k_quads`
- `intercept_ms`
- `r_squared`
- `number_of_points`

This CSV is the operational check for “scales predictably.”

## Validation Commands

Recommended correctness and build validation:

```bash
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo bench --no-run
```

Recommended smoke validation:

```bash
cargo run --release --bin paper_hybrid_coordination -- \
  --warmup-runs 1 --runs 2 --historical-events 1000 --live-events 8 --mode warm \
  --output-dir target/paper_benchmarks/validation_h1

cargo run --release --bin paper_historical_scaling -- \
  --warmup-runs 1 --runs 2 --dataset-sizes 10000,50000,100000 --mode warm \
  --output-dir target/paper_benchmarks/validation_h2
```

## Query-Defined Baseline Smoke Benchmark

This runner exercises the query-defined baseline path with a short, deterministic fixture:

- historical baseline query execution is always accelerated
- GRAPH-template materialization
- static quad injection
- live startup
- first-result latency
- live-only comparison with the same live window and aggregate
- live replay can run in accelerated mode, which remains the default, or realtime mode for wall-clock sanity checks

Binary:

- `paper_query_defined_baseline`

Default smoke command:

```bash
cargo run --release --bin paper_query_defined_baseline -- \
  --warmup-runs 0 --runs 1
```

Live replay controls:

- `--live-replay-mode accelerated|realtime`
- `--live-rate-hz <float>`
- `--live-duration-seconds <u64>`
- `--live-window-size-seconds <u64>`
- `--live-window-slide-seconds <u64>`

Behavior notes:

- historical preload and historical baseline evaluation always stay accelerated
- accelerated live replay preserves the current benchmark behavior and does not add sleeps
- realtime live replay sleeps only between live events, so it is suitable for short live-arrival sanity checks
- the realtime path is not intended for million-scale historical experiments; keep live replay accelerated unless you are explicitly testing wall-clock behavior
- when realtime mode is selected without explicit window values, the runner uses a 240-second live duration with 120-second windows sliding every 60 seconds

The runner writes a timestamped directory under `logs/benchmark/query_defined_baseline/` containing:

- `query_defined_baseline.raw.json`
- `query_defined_baseline.summary.csv`
- `query_defined_baseline_results.md`

Longer run after smoke passes:

```bash
cargo run --release --bin paper_query_defined_baseline -- \
  --warmup-runs 1 --runs 25
```

Note:

- the smoke fixture keeps the historical window fixed so the default run stays short and deterministic
- the smoke runner uses the tested `ex:` alias surface and a conservative live query shape so it stays within the current runtime parse path
- if you want to stretch the benchmark, increase `--runs` first before expanding the fixture size
- the benchmark verifies `?sensor`, `?minuteAvgValue`, `?dayAvgValue`, and `?difference` on the baseline path and compares against a live-only query with the same live window
- realtime mode records `expected_emitted_windows`, `expected_full_windows`, `warmup_window_count`, observed emitted-window counts, observed row counts, and per-window latency values when available
- realtime mode may emit an initial partial or warm-up window on the first slide, depending on engine boundary semantics

## Recommended Final Paper Commands

```bash
cargo run --release --bin paper_hybrid_coordination -- \
  --warmup-runs 1 --runs 10 --historical-events 10000 --live-events 32 --mode warm \
  --output-dir target/paper_benchmarks/paper_h1_final

cargo run --release --bin paper_historical_scaling -- \
  --warmup-runs 1 --runs 5 --dataset-sizes 100000,500000,1000000,5000000 --mode warm \
  --output-dir target/paper_benchmarks/paper_h2_final
```

For final numbers:

1. run on a quiet machine
2. keep the emitted raw JSONL with the paper tables
3. retain commit SHA, branch, OS, CPU, RAM, and exact command line
