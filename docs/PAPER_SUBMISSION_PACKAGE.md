# Paper Submission Package

This document lists what should and should not be included in a demo or paper submission package.

## Files To Include

- `README.md`
- `docs/README.md`
- `docs/DOCUMENTATION_INDEX.md`
- `docs/JANUSQL.md`
- `docs/QUERY_EXECUTION.md`
- `docs/BASELINES.md`
- `docs/NESTED_HISTORICAL_SUBQUERIES.md`
- `docs/PAPER_BENCHMARKING.md`
- `docs/PAPER_ARTIFACT_MAP.md`
- `docs/PAPER_ARCHITECTURE.md`
- `docs/PAPER_BENCHMARK_RESULTS_TEMPLATE.md`
- `docs/PAPER_SUBMISSION_PACKAGE.md`
- `scripts/plot_janus_benchmark_results.py`
- `target/paper_benchmarks/figures/20260619_110106/historical_scaling_p50.png`
- `target/paper_benchmarks/figures/20260619_110106/historical_scaling_p50.pdf`
- `target/paper_benchmarks/figures/20260619_110106/hybrid_coordination_latency.png`
- `target/paper_benchmarks/figures/20260619_110106/hybrid_coordination_latency.pdf`
- `target/paper_benchmarks/figures/20260619_110106/hybrid_coordination_transfer.png`
- `target/paper_benchmarks/figures/20260619_110106/hybrid_coordination_transfer.pdf`
- `target/paper_benchmarks/figures/20260619_110106/janus_benchmark_tables.md`

## Files To Exclude

- raw benchmark CSV files
- raw benchmark JSONL files
- generated `citybench_*.nq` datasets
- scratch benchmark directories such as `debug_*`, `smoke_*`, `validation_*`, and `pre_final_*`
- local benchmark summaries intended only for drafting or verification

## Benchmark Artifacts To Preserve

- `target/paper_benchmarks/historical_materialized_subquery/20260619_110106/historical_materialized_subquery.md`
- `target/paper_benchmarks/paper_hybrid_coordination/20260619_110106/paper_hybrid_coordination.summary.csv`
- `target/paper_benchmarks/paper_hybrid_coordination/20260619_110106/paper_hybrid_coordination.raw.jsonl`
- `target/paper_benchmarks/paper_historical_scaling_full/20260619_110323/paper_historical_scaling.summary.csv`
- `target/paper_benchmarks/paper_historical_scaling_full/20260619_110323/paper_historical_scaling.raw.jsonl`
- `target/paper_benchmarks/paper_historical_scaling_full/20260619_110323/paper_historical_scaling.fit.csv`

## Benchmark Artifacts To Keep Local Only

- `target/paper_benchmarks/paper_hybrid_coordination/20260619_110106/paper_hybrid_coordination.summary.csv`
- `target/paper_benchmarks/paper_hybrid_coordination/20260619_110106/paper_hybrid_coordination.raw.jsonl`
- `target/paper_benchmarks/paper_historical_scaling_full/20260619_110323/paper_historical_scaling.summary.csv`
- `target/paper_benchmarks/paper_historical_scaling_full/20260619_110323/paper_historical_scaling.raw.jsonl`
- `target/paper_benchmarks/paper_historical_scaling_full/20260619_110323/paper_historical_scaling.fit.csv`
- `target/paper_benchmarks/paper_historical_scaling_full/20260619_110323/logs/citybench_100000.nq`
- `target/paper_benchmarks/paper_historical_scaling_full/20260619_110323/logs/citybench_500000.nq`
- `target/paper_benchmarks/paper_historical_scaling_full/20260619_110323/logs/citybench_1000000.nq`
- `target/paper_benchmarks/paper_historical_scaling_full/20260619_110323/logs/citybench_5000000.nq`
- all scratch benchmark directories and historical validation runs under `target/paper_benchmarks/`
