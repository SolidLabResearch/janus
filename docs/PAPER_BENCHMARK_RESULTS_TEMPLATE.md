# Paper Benchmark Results Template

## H1.1 Coordination Overhead

| System | Mode | Runs | Components | Process Boundaries | Serialization Steps | P50 E2E Latency (ms) | P95 E2E Latency (ms) | Avg Useful Engine Work (ms) | Avg Coordination Overhead (ms) | Avg External Transfer Bytes | Avg Final Result Bytes | Avg Result Count |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Janus unified | warm |  |  |  |  |  |  |  |  |  |  |  |
| Decomposed Oxigraph | warm |  |  |  |  |  |  |  |  |  |  |  |

## H2 Scaling

| Dataset Size (quads) | Query Type | Mode | Runs | Logical Quads Scanned | Selectivity | Result Count | P50 Latency (ms) | P95 Latency (ms) | Avg Latency (ms) | Avg Throughput (quads/sec) | Peak RSS (MB) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 100,000 | point_lookup | warm |  |  |  |  |  |  |  |  |  |
| 100,000 | fixed_window | warm |  |  |  |  |  |  |  |  |  |
| 100,000 | proportional_range_10 | warm |  |  |  |  |  |  |  |  |  |
| 100,000 | proportional_range_50 | warm |  |  |  |  |  |  |  |  |  |
| 100,000 | full_range | warm |  |  |  |  |  |  |  |  |  |
| 100,000 | hybrid_baseline_lookup | warm |  |  |  |  |  |  |  |  |  |
| 500,000 | point_lookup | warm |  |  |  |  |  |  |  |  |  |
| 500,000 | fixed_window | warm |  |  |  |  |  |  |  |  |  |
| 500,000 | proportional_range_10 | warm |  |  |  |  |  |  |  |  |  |
| 500,000 | proportional_range_50 | warm |  |  |  |  |  |  |  |  |  |
| 500,000 | full_range | warm |  |  |  |  |  |  |  |  |  |
| 500,000 | hybrid_baseline_lookup | warm |  |  |  |  |  |  |  |  |  |
| 1,000,000 | point_lookup | warm |  |  |  |  |  |  |  |  |  |
| 1,000,000 | fixed_window | warm |  |  |  |  |  |  |  |  |  |
| 1,000,000 | proportional_range_10 | warm |  |  |  |  |  |  |  |  |  |
| 1,000,000 | proportional_range_50 | warm |  |  |  |  |  |  |  |  |  |
| 1,000,000 | full_range | warm |  |  |  |  |  |  |  |  |  |
| 1,000,000 | hybrid_baseline_lookup | warm |  |  |  |  |  |  |  |  |  |
| 5,000,000 | point_lookup | warm |  |  |  |  |  |  |  |  |  |
| 5,000,000 | fixed_window | warm |  |  |  |  |  |  |  |  |  |
| 5,000,000 | proportional_range_10 | warm |  |  |  |  |  |  |  |  |  |
| 5,000,000 | proportional_range_50 | warm |  |  |  |  |  |  |  |  |  |
| 5,000,000 | full_range | warm |  |  |  |  |  |  |  |  |  |
| 5,000,000 | hybrid_baseline_lookup | warm |  |  |  |  |  |  |  |  |  |

## H2 Scaling Fit

| Query Type | Mode | Slope (ms / 100k quads) | Intercept (ms) | R² | Number Of Points |
| --- | --- | --- | --- | --- | --- |
| point_lookup | warm |  |  |  |  |
| fixed_window | warm |  |  |  |  |
| proportional_range_10 | warm |  |  |  |  |
| proportional_range_50 | warm |  |  |  |  |
| full_range | warm |  |  |  |  |
| hybrid_baseline_lookup | warm |  |  |  |  |
