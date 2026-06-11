# Paper Benchmark Results Template

## H1.1 Hybrid Query Performance

| System | Mode | Historical Latency (ms) | Live Stream Processing Latency (ms) | External Join Latency (ms) | First Hybrid Result Latency (ms) | External Transfer Bytes | Historical Equivalence | Hybrid Equivalence |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Janus unified | warm | | | | | | | |
| Oxigraph historical + Janus live window processor + external join | warm | | | | | | | |

## H1.2 Sustained Hybrid Query Performance

| System | Mode | Time Mode | Runs | Logical Live Duration (s) | Event Rate (Hz) | Event Interval (ms) | Window Size (ms) | Window Slide (ms) | Completed Windows Total | Completed Windows In Horizon | Flush Windows | Missed Windows | Historical Prep Latency (ms) | P50 Window Hybrid Latency (ms) | P95 Window Hybrid Latency (ms) | P50 Window Result Wall Clock Offset (ms) | P95 Window Result Wall Clock Offset (ms) | Avg External Join Latency (ms) | Avg External Transfer Bytes / Window | Equivalence Rate | Avg Wall Clock Duration (ms) | Uses Virtual Event Time |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Janus unified | warm | virtual | | | | | | | | | | | | | | | | | | | | |
| Oxigraph historical + Janus live window processor + external join | warm | virtual | | | | | | | | | | | | | | | | | | | | |

Paper-facing H1.2 latency, transfer, and equivalence metrics are computed over completed windows in horizon only. Flush windows are tracked separately and excluded from those summaries.

## H1.2b Wall-Clock Sanity Check

| System | Mode | Time Mode | Live Duration (s) | Event Rate (Hz) | Window Size (s) | Window Slide (s) | Completed Windows Total | Completed Windows In Horizon | Flush Windows | First Hybrid Result Wall Clock (ms) | P50 Window Result Wall Clock Offset (ms) | P95 Window Result Wall Clock Offset (ms) | Avg Wall Clock Duration (ms) | Avg Wall Clock Overhead (ms) | Equivalence Rate |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Janus unified | warm | wall-clock | | | | | | | | | | | | | |
| Oxigraph historical + Janus live window processor + external join | warm | wall-clock | | | | | | | | | | | | | |


## H2 Historical Scaling Result

Lead with simple latency values, not slope/R². Use `p50_latency_ms` as the main number.

If the selected H2 output directory does not include `5,000,000` quads, label the table as validation-only and replace the size columns with the available dataset sizes.

## H2 Simple Latency Table

| query_type | latency_100k_ms | latency_500k_ms | latency_1m_ms | latency_5m_ms | simple_takeaway |
| --- | --- | --- | --- | --- | --- |
| point_lookup |  |  |  |  | flat |
| fixed_window |  |  |  |  | almost flat |
| proportional_range_10 |  |  |  |  | grows moderately |
| proportional_range_50 |  |  |  |  | grows strongly |
| full_range |  |  |  |  | largest scan cost |
| hybrid_baseline_lookup |  |  |  |  | largest scan cost |

## H2 Optional Latency Growth Table

| query_type | latency_increase_from_smallest_to_largest_ms | multiplier_from_smallest_to_largest | simple_interpretation |
| --- | --- | --- | --- |
| point_lookup |  |  | Point lookup barely changes |
| fixed_window |  |  | Fixed-window query stays nearly flat |
| proportional_range_10 |  |  | 10% range grows moderately as it scans more data |
| proportional_range_50 |  |  | 50% range grows strongly as it scans more data |
| full_range |  |  | Full range grows because it scans more data |
| hybrid_baseline_lookup |  |  | Hybrid baseline follows full-range scan behavior |

## H2 Plot

Reference:

- `target/paper_benchmarks/h2_latency_scaling.png`
- `target/paper_benchmarks/h2_latency_scaling_logy.png` when the flatter query lines are hard to read on a linear y-axis

## H2 Scaling Fit Appendix

Retain the fit output as appendix/internal support, not the primary result.

| Query Type | Mode | Slope (ms / 100k quads) | Intercept (ms) | R² | Number Of Points |
| --- | --- | --- | --- | --- | --- |
| point_lookup | warm |  |  |  |  |
| fixed_window | warm |  |  |  |  |
| proportional_range_10 | warm |  |  |  |  |
| proportional_range_50 | warm |  |  |  |  |
| full_range | warm |  |  |  |  |
| hybrid_baseline_lookup | warm |  |  |  |  |
