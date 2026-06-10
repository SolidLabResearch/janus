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
