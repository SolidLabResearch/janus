# Paper Artifact Map

This document maps each paper claim to the benchmark evidence, output files, and intended paper destination.

## Historical Materialized Subquery Claim

| Paper claim | Benchmark | Output files | Figure/table destination | Notes and caveats |
| --- | --- | --- | --- | --- |
| Historical materialized subqueries lower to the historical materialization path and behave similarly to explicit `DEFINE BASELINE` | Historical Materialized Subquery Benchmark | [`target/paper_benchmarks/historical_materialized_subquery/20260619_110106/historical_materialized_subquery.md`](../target/paper_benchmarks/historical_materialized_subquery/20260619_110106/historical_materialized_subquery.md) | Appendix table for explicit vs nested historical subquery comparison | `baseline_bindings` is a sanity check, not a claim metric. `first_result_latency_ms_avg` is `n/a` in this benchmark. |

## Unified Hybrid Execution Claim

| Paper claim | Benchmark | Output files | Figure/table destination | Notes and caveats |
| --- | --- | --- | --- | --- |
| Unified hybrid execution reduces coordination complexity and avoids external transfer relative to the decomposed baseline | Hybrid Coordination Benchmark | [`target/paper_benchmarks/paper_hybrid_coordination/20260619_110106/paper_hybrid_coordination.summary.csv`](../target/paper_benchmarks/paper_hybrid_coordination/20260619_110106/paper_hybrid_coordination.summary.csv), [`target/paper_benchmarks/paper_hybrid_coordination/20260619_110106/paper_hybrid_coordination.raw.jsonl`](../target/paper_benchmarks/paper_hybrid_coordination/20260619_110106/paper_hybrid_coordination.raw.jsonl) | Figure 2: unified vs decomposed hybrid execution latency; Figure 3: external transfer; main coordination table | The decomposed row label in raw output is implementation-specific. `historical_equivalence_rate` and `hybrid_equivalence_rate` for the decomposed row are schema/design zeros and should not be interpreted as failures. |

## Historical Scaling Claim

| Paper claim | Benchmark | Output files | Figure/table destination | Notes and caveats |
| --- | --- | --- | --- | --- |
| Historical query latency changes with dataset size, with point lookups and fixed windows remaining nearly flat while scan-heavy queries grow strongly | Historical Scaling Benchmark | [`target/paper_benchmarks/paper_historical_scaling_full/20260619_110323/paper_historical_scaling.summary.csv`](../target/paper_benchmarks/paper_historical_scaling_full/20260619_110323/paper_historical_scaling.summary.csv), [`target/paper_benchmarks/paper_historical_scaling_full/20260619_110323/paper_historical_scaling.raw.jsonl`](../target/paper_benchmarks/paper_historical_scaling_full/20260619_110323/paper_historical_scaling.raw.jsonl), [`target/paper_benchmarks/paper_historical_scaling_full/20260619_110323/paper_historical_scaling.fit.csv`](../target/paper_benchmarks/paper_historical_scaling_full/20260619_110323/paper_historical_scaling.fit.csv) | Figure 1: historical query latency by dataset size; main/appendix historical scaling table | `hybrid_baseline_lookup` is the noisiest query at 5M quads. The fit CSV is supporting evidence, not the primary result. |

## Output Package Notes

- Keep the figure outputs and compact tables for the submission package.
- Keep raw CSV, JSONL, fit CSV, and generated `citybench_*.nq` datasets local only.
- Use the benchmark summaries and tables for review; do not cite raw artifacts directly in the paper unless needed for appendix verification.
