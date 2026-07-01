# Hybrid Scaling Combined Benchmark

This directory contains output results from running the hybrid scaling combined benchmark.

## Files

- `hybrid_scaling_combined.raw.jsonl`: Raw per-run JSON lines log
- `hybrid_scaling_combined.summary.csv`: Summary CSV grouped by historical size and system
- `hybrid_scaling_combined_results.md`: Summary Markdown tables ready for review

## How to Run

```bash
cargo run --release --bin hybrid_scaling_combined -- --historical-sizes 10000,50000,100000,500000 --historical-query-types point_lookup,fixed_60s,range_10_percent,range_50_percent,range_100_percent --iterations 5 --live-duration-ms 20000 --event-rate 4 --event-interval-ms 250 --window-size-ms 10000 --window-slide-ms 5000
```
