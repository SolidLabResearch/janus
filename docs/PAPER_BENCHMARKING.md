# Paper Benchmarking

Paper-facing benchmarks are executable binaries. Each writes raw output and
summaries to its output directory; preserve the directory, command, Git SHA,
and machine metadata as one result package.

## Maintained binaries

| Binary | Purpose | Primary outputs |
| --- | --- | --- |
| `paper_hybrid_coordination` | Unified Janus versus decomposed Oxigraph coordination workload | `paper_hybrid_coordination.raw.jsonl`, summary CSV |
| `paper_historical_scaling` | Historical-access scaling over multiple data sizes and query types | raw JSONL, summary CSV, fit CSV |
| `paper_sustained_hybrid` | Sustained unified/decomposed live-window workload | raw JSONL and summary CSV |
| `paper_historical_range_comparison` | Fixed 60-second and full-history comparisons | raw JSONL, summary CSV, Markdown, plots |
| `historical_materialized_subquery_benchmark` | Historical materialization path | benchmark-specific report artifacts |
| `storage_footprint_benchmark` | Janus/Oxigraph persistent storage footprint | raw CSV, summary CSV, ratio CSV, Markdown |

## Commands

Inspect the exact accepted arguments in the checked-out revision:

```bash
cargo run --release --bin paper_hybrid_coordination -- --help
cargo run --release --bin paper_historical_scaling -- --help
cargo run --release --bin paper_sustained_hybrid -- --help
cargo run --release --bin paper_historical_range_comparison -- --help
cargo run --release --bin historical_materialized_subquery_benchmark -- --help
cargo run --release --bin storage_footprint_benchmark -- --help
```

Use an explicit, new output directory for a result package:

```bash
cargo run --release --bin storage_footprint_benchmark -- \
  --event-counts 10000,50000,100000,1000000 \
  --iterations 5 \
  --output-dir results/storage_footprint_$(git rev-parse --short HEAD)
```

The ten-million-event footprint case is opt-in through `--include-10m`.

## Evidence requirements

- Do not compare results from different machines, revisions, data generators,
  or execution modes as one series without clearly labeling the difference.
- Keep warm-up rows separate from measured rows unless the report says
  otherwise.
- Keep correctness/equivalence evidence with performance rows.
- Treat absent work as `N/A`, not as zero cost.
- A generated plot or a summary CSV without raw rows and provenance is not a
  standalone paper claim.

The implemented CityBench-inspired scaling workload is documented in
[benchmarks/citybench_congestion_hybrid_scaling.md](./benchmarks/citybench_congestion_hybrid_scaling.md).
