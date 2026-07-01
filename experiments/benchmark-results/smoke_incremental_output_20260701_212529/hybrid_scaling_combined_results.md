# Hybrid Scaling Combined Benchmark Results

## Table 1: End-to-end hybrid latency

| Historical query type | Historical size | System | Historical backend | Historical query language | First hybrid result mean ± std | Main window result mean ± std | Historical query mean ± std | External merge mean ± std | Result equivalence |
|---|---:|---|---|---|---:|---:|---:|---:|---|
| point_lookup | 10000 | Janus Unified | janus_segmented | janus_range_lookup | 0.000 ± 0.000 ms | 0.000 ± 0.000 ms | 0.000 ± 0.000 ms | 0.000 ± 0.000 ms | 100% |
| point_lookup | 10000 | Decomposed-Oxigraph | oxigraph | sparql_filter | 2753.609 ± 0.000 ms | 4778.157 ± 0.000 ms | 4.000 ± 0.000 ms | 0.015 ± 0.000 ms | 100% |

## Table 2: Historical access scaling inside the combined benchmark

| Historical query type | System | 10k quads | 50k quads | 100k quads | 500k quads | Takeaway |
|---|---|---:|---:|---:|---:|---|
| point_lookup | Janus Unified | 0.000 ms | N/A | N/A | N/A | flat |
| point_lookup | Decomposed-Oxigraph | 4.000 ms | N/A | N/A | N/A | flat |

## Table 3: Historical result counts

| Historical query type | Historical size | System | Historical result count mean ± std |
|---|---:|---|---:|
| point_lookup | 10000 | Janus Unified | 1.000 ± 0.000 |
| point_lookup | 10000 | Decomposed-Oxigraph | 1.000 ± 0.000 |

## Table 4: Result equivalence

| Historical query type | Historical size | Janus result count | Decomposed result count | Hash equivalence |
|---|---:|---|---|---|
| point_lookup | 10000 | 0.0 | 0.0 | 100% |

## Table 5: Process-level resource utilization

| Historical query type | Historical size | System | Peak RSS MB mean ± std | RSS delta MB mean ± std | Mean CPU % mean ± std | Peak CPU % mean ± std |
|---|---:|---|---:|---:|---:|---:|
| point_lookup | 10000 | Janus Unified | 23.844 ± 0.000 MB | 2.828 ± 0.000 MB | 2.018 ± 0.000% | 8.352 ± 0.000% |
| point_lookup | 10000 | Decomposed-Oxigraph | 45.406 ± 0.000 MB | 20.828 ± 0.000 MB | 1.585 ± 0.000% | 8.219 ± 0.000% |

### Documentation Notes

- **Process-Level Resource Measurement Limitation**: Resource measurements are process-level measurements collected during each run. In a single long-running process, RSS can be affected by allocator retention and previous configurations. For fully isolated memory comparison, each configuration should be run in a fresh process.

- **Decomposed-Oxigraph Baseline**: Decomposed-Oxigraph evaluates the historical component using SPARQL over the full Oxigraph historical store and merges the resulting historical bindings with separately evaluated live results. It does not use Janus segmented historical lookup for historical filtering.

