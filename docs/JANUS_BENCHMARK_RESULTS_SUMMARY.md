# Janus Benchmark Results Summary

All numbers below are copied from the completed benchmark outputs in `target/paper_benchmarks/` and are intended for engineering and research reference, not as publication text.

## Benchmark Environment

- git SHA: `0a7872d49d76af36a336de128d931bc34e01d7e5`
- branch: `main`
- rustc version: `rustc 1.90.0 (1159e78c4 2025-09-14)`
- cargo version: `cargo 1.90.0 (840b83a10 2025-07-30)`
- operating system: `Darwin 25.5.0` / `Darwin Kernel Version 25.5.0`
- CPU: `Apple M4`
- RAM: `17179869184` bytes (`16 GiB`)
- benchmark date: `2026-06-19`
- release build confirmation: yes, all runs were executed with `cargo run --release`

## Benchmark 1: Historical Materialized Subqueries

### Goal

- explicit `DEFINE BASELINE`
- nested historical subquery
- historical materialization path

### Configuration

- benchmark: `nested_historical_subquery`
- historical events: `5000`
- entity count: `5`
- runs: `5`

### Results

| Query form | parse_total_ms_avg | planning_lowering_ms_avg | historical_materialization_ms_avg | live_startup_ms_avg | baseline_bindings |
| --- | ---: | ---: | ---: | ---: | ---: |
| explicit baseline | 0.038 | 0.003 | 5.753 | 0.286 | 5.0 |
| nested historical subquery | 0.031 | 0.019 | 5.546 | 0.205 | 5.0 |

### Observations

- Binding equivalence held: both query forms produced `baseline_bindings=5.0`.
- The nested query used the historical materialization path, with `Execution mode: HistoricalMaterializedOnce` and `Physical plan: MaterializeHistoricalResult`.
- Timing differences are small relative to the overall benchmark and should be treated as near-parity measurements rather than a strong performance separation.
- `first_result_latency_ms_avg` was `n/a` in this benchmark, so it should not be used as a comparison metric here.

## Benchmark 2: Hybrid Coordination

### Goal

- unified execution
- decomposed execution

### Configuration

- historical events: `10000`
- live events: `32`
- warmups: `1`
- measured runs: `10`
- mode: `warm`

### Results

| System | p50_e2e_latency_ms | p95_e2e_latency_ms | avg_coordination_overhead_ms | avg_external_transfer_bytes | components | process_boundaries | serialization_steps |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| janus_unified | 22.000 | 23.000 | 10.500 | 0.000 | 1 | 0 | 1 |
| decomposed (`Oxigraph historical + Janus live window processor + external join`) | 27.000 | 27.000 | 10.300 | 79758.000 | 4 | 3 | 4 |

### Observations

- Unified execution reduced end-to-end latency from `27.000 ms` to `22.000 ms` at p50.
- The decomposed path moved `79758` bytes externally, while the unified path moved `0`.
- Coordination structure was materially simpler in the unified case: `1` component, `0` process boundaries, and `1` serialization step versus `4`, `3`, and `4`.
- Equivalence checks should be read carefully: the unified row reports `historical_equivalence_rate=1.000` and `hybrid_equivalence_rate=1.000`, while the decomposed row reports `0.000` for both by schema/design in the benchmark output and should not be treated as a correctness failure.

## Benchmark 3: Historical Scaling

### Goal

- scaling historical query execution
- increasing historical dataset size

### Configuration

- dataset sizes: `100000`, `500000`, `1000000`, `5000000`
- query types: `point_lookup`, `fixed_window`, `proportional_range_10`, `proportional_range_50`, `full_range`, `hybrid_baseline_lookup`
- runs: `5`
- warmups: `1`
- mode: `warm`

### Results

| Query type | Smallest dataset latency | Largest dataset latency | Overall trend |
| --- | ---: | ---: | --- |
| point_lookup | 0.452 ms at 100000 quads | 0.370 ms at 5000000 quads | Flat to slightly decreasing |
| fixed_window | 2.624 ms at 100000 quads | 2.805 ms at 5000000 quads | Nearly flat |
| proportional_range_10 | 14.259 ms at 100000 quads | 652.331 ms at 5000000 quads | Strong growth with dataset size |
| proportional_range_50 | 64.323 ms at 100000 quads | 3302.658 ms at 5000000 quads | Very strong growth with dataset size |
| full_range | 128.911 ms at 100000 quads | 7277.713 ms at 5000000 quads | Steep scan-heavy growth |
| hybrid_baseline_lookup | 137.604 ms at 100000 quads | 7688.995 ms at 5000000 quads | Steep scan-heavy growth, highest variance |

### Observations

- Point lookup behavior stayed sub-millisecond across all sizes and remained the flattest query shape.
- Fixed-window latency was almost flat, suggesting modest sensitivity to historical growth for that access pattern.
- Scan-heavy query types grew sharply as dataset size increased, with the 50% range and full-range queries showing the steepest slopes.
- Peak RSS increased with dataset size across query types, reaching `2911.547 MB` for `full_range` and `2865.484 MB` for `hybrid_baseline_lookup` at `5000000` quads.
- Variance was most notable for `hybrid_baseline_lookup` at `5000000` quads, where `p95_latency_ms=13857.199` versus `p50_latency_ms=7688.995`.

## Key Takeaways

- Historical materialized subqueries behaved as expected and used the historical materialization execution path.
- Unified hybrid execution reduced coordination complexity and eliminated external transfer in the measured run.
- Historical execution showed clear size sensitivity for scan-heavy queries and near-flat behavior for point lookups and fixed windows.
- The main limitations in these outputs are benchmark-specific: the nested subquery benchmark does not produce a first-result latency, the hybrid decomposition equivalence fields are schema-driven zeros, and the historical scaling fit output is an auxiliary estimate rather than the primary result.

## Caveats

- `baseline_bindings` is a sanity metric for the nested-subquery benchmark, not a paper-facing performance result.
- `historical_equivalence_rate` and `hybrid_equivalence_rate` in the decomposed hybrid row are not evidence of incorrectness; they are output-format artifacts and should not be over-interpreted.
- `avg_throughput_quads_per_sec`, `max_peak_rss_mb`, and the fit coefficients in `paper_historical_scaling.fit.csv` are useful support metrics, but the primary story should use latency by dataset size.
- `hybrid_baseline_lookup` is the noisiest historical-scaling query at the largest dataset size and should be treated cautiously if used in a figure.
- `point_lookup` has a low `R²` in the fit output because its latency is nearly flat; the fit is therefore a weak descriptive model for that query type.

## Artifacts

### Output directories

- `/Users/kushbisen/Code/janus/target/paper_benchmarks/historical_materialized_subquery/20260619_110106`
- `/Users/kushbisen/Code/janus/target/paper_benchmarks/paper_hybrid_coordination/20260619_110106`
- `/Users/kushbisen/Code/janus/target/paper_benchmarks/paper_historical_scaling_full/20260619_110323`

### Files

- `/Users/kushbisen/Code/janus/target/paper_benchmarks/historical_materialized_subquery/20260619_110106/historical_materialized_subquery.md`
- `/Users/kushbisen/Code/janus/target/paper_benchmarks/paper_hybrid_coordination/20260619_110106/paper_hybrid_coordination.summary.csv`
- `/Users/kushbisen/Code/janus/target/paper_benchmarks/paper_hybrid_coordination/20260619_110106/paper_hybrid_coordination.raw.jsonl`
- `/Users/kushbisen/Code/janus/target/paper_benchmarks/paper_historical_scaling_full/20260619_110323/paper_historical_scaling.summary.csv`
- `/Users/kushbisen/Code/janus/target/paper_benchmarks/paper_historical_scaling_full/20260619_110323/paper_historical_scaling.raw.jsonl`
- `/Users/kushbisen/Code/janus/target/paper_benchmarks/paper_historical_scaling_full/20260619_110323/paper_historical_scaling.fit.csv`

### Generated datasets

- `/Users/kushbisen/Code/janus/target/paper_benchmarks/paper_historical_scaling_full/20260619_110323/logs/citybench_100000.nq`
- `/Users/kushbisen/Code/janus/target/paper_benchmarks/paper_historical_scaling_full/20260619_110323/logs/citybench_500000.nq`
- `/Users/kushbisen/Code/janus/target/paper_benchmarks/paper_historical_scaling_full/20260619_110323/logs/citybench_1000000.nq`
- `/Users/kushbisen/Code/janus/target/paper_benchmarks/paper_historical_scaling_full/20260619_110323/logs/citybench_5000000.nq`
