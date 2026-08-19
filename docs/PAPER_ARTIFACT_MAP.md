# Paper Artifact Map

This map defines evidence requirements for paper-facing Janus claims. It does
not designate a particular local `target/` directory as authoritative.

| Claim area | Executable source | Required evidence |
| --- | --- | --- |
| Historical access latency | `hybrid_scaling_combined` | Raw per-iteration rows, summary, query type, dataset size, machine metadata, command, Git SHA, and result-equivalence checks. |
| Unified versus decomposed hybrid execution | `paper_hybrid_coordination` or `hybrid_scaling_combined` | Both approaches' raw rows, comparable configuration, transfer/latency definitions, correctness evidence, and `N/A` for inapplicable steps. |
| Historical materialization | `historical_materialized_subquery_benchmark` | Query fixtures, raw result rows, parser/lowering evidence, and clearly stated supported shape. |
| Storage footprint | `storage_footprint_benchmark` | Per-run CSV, event counts, iterations, storage directories measured, cleanup policy, machine metadata, and command. |

Generated figures are presentation artifacts, not the only evidence. A result
is safe to cite only when the raw files and the exact run configuration are
available. See [PAPER_BENCHMARKING.md](./PAPER_BENCHMARKING.md) and
[the CityBench benchmark guide](./benchmarks/citybench_congestion_hybrid_scaling.md).
