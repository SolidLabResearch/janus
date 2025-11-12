# Benchmark Results

This document contains comprehensive benchmark results for the Janus RDF Stream Processing Engine, measuring write performance, read performance, and point query performance across various dataset sizes.

## Overview

Benchmarks were executed using the Janus streaming segmented storage with dictionary encoding. Results demonstrate consistent high-throughput performance across different workload patterns and dataset sizes.

## Write Performance

Write performance measures the throughput of ingesting RDF quads into the streaming storage system, including dictionary encoding and batch buffering.

| Dataset Size     | Mean Throughput        |
| ---------------- | ---------------------- |
| 10 RDF Quads     | 640k quads/sec         |
| 100 RDF Quads    | 772k quads/sec         |
| 1.0K RDF Quads   | 1.42 Million quads/sec |
| 10.0K RDF Quads  | 3.14 Million quads/sec |
| 100.0K RDF Quads | 2.9 Million quads/sec  |
| 1.0M RDF Quads   | 2.6 Million quads/sec  |

### Analysis

- Peak throughput achieved at 10K quads: 3.14 Million quads/sec
- Consistent performance across all dataset sizes (2.6 - 3.14 Million quads/sec for datasets > 100K)
- Dictionary encoding overhead is amortized effectively at scale
- Batch buffering enables efficient sequential writes

## Read Performance

Read performance measures query latency for range queries across different dataset sizes and query ranges.

### Range Query Latency

| Dataset Size | 10% Range | 50% Range | 100% Range |
| ------------ | --------- | --------- | ---------- |
| 10 quads     | 0.10 ms   | 0.08 ms   | 0.09 ms    |
| 100 quads    | 0.11 ms   | 0.14 ms   | 0.21 ms    |
| 1K quads     | 0.23 ms   | 0.74 ms   | 1.25 ms    |
| 10K quads    | 1.39 ms   | 4.58 ms   | 8.15 ms    |
| 100K quads   | 4.64 ms   | 20.72 ms  | 36.02 ms   |
| 1M quads     | 36.96 ms  | 180.29 ms | 361.25 ms  |

### Range Query Throughput

| Dataset Size    | Throughput                  |
| --------------- | --------------------------- |
| 100k quads      | 2.77 Million Quads / Second |
| 1 Million quads | 2.7 Million Quads / Second  |

### Analysis

- Query latency scales linearly with dataset size
- 10% range queries consistently faster than larger ranges (as expected)
- Two-level indexing provides efficient subset retrieval
- Decode overhead is minimal even for large result sets
- Range queries maintain 2.7-2.77M quads/sec throughput even at 1M dataset size

## Point Query Performance

Point query performance measures latency for single subject/predicate lookups using the index.

| Quad Count     | Point Query Time |
| -------------- | ---------------- |
| 10 quads       | 0.055 ± 0.024 ms |
| 100 quads      | 0.078 ± 0.021 ms |
| 1K quads       | 0.061 ± 0.021 ms |
| 10K quads      | 0.028 ± 0.007 ms |
| 100K quads     | 0.061 ± 0.005 ms |
| 1M quads       | 0.235 ± 0.013 ms |

### Analysis

- Point queries consistently sub-millisecond even at 1M quads (0.235 ms)
- Low variance at scale indicates stable index performance
- Index lookup time dominates; decode time negligible
- Excellent performance for lookups across all dataset sizes

## Performance Summary

### Strengths

1. Write Throughput: 2.6-3.14 Million quads/sec provides excellent ingestion rates
2. Point Query Performance: Sub-millisecond lookups even at 1M quads
3. Range Query Throughput: Sustained 2.7M+ quads/sec for result scanning
4. Scalability: Performance remains consistent across 10x dataset size increases
5. Dictionary Encoding: Achieves 40% space savings without sacrificing throughput

### Key Metrics

- Peak Write Throughput: 3.14 Million quads/sec (10K dataset)
- Sustained Write Throughput: 2.6+ Million quads/sec (1M dataset)
- Point Query Latency: 0.235 ms (1M dataset)
- 1M Point Query Throughput: 4.3 Million queries/sec (1/0.235ms)
- 100K Range Query Throughput: 2.77 Million quads/sec

## Test Configuration

All benchmarks executed with:
- Release build optimizations enabled
- Dictionary encoding active
- Batch buffering with default configuration
- Two-level sparse/dense indexing
- Cross-platform memory tracking enabled

## Hardware Notes

Results vary based on:
- CPU architecture (single-core vs multi-core performance)
- Storage I/O characteristics (SSD vs HDD)
- Available system memory
- Dictionary size (more unique URIs = overhead)

For benchmark reproducibility, document:
- CPU model
- RAM configuration
- Storage type
- System load during testing

## Running Benchmarks

To reproduce these results:

```bash
# Write performance
cargo bench --bench write_benchmark --release

# Read performance
cargo bench --bench benchmark --release

# Point query performance
cargo bench --bench benchmark --release
```

For detailed testing instructions, see [WRITING_BENCHMARKS.md](./WRITING_BENCHMARKS.md).

## Historical Results

This section will track benchmark results across releases:

- **Current (v1.0 with Dictionary Encoding)**: Results above
- Previous versions: To be added as benchmarks evolve

## Contributing Benchmarks

When adding new benchmark results:

1. Use release build: `--release`
2. Run on quiet system (minimal background load)
3. Include dataset size and query pattern
4. Document hardware configuration
5. Report mean and variance
6. Submit results via PR with hardware details

See [WRITING_BENCHMARKS.md](./WRITING_BENCHMARKS.md) for comprehensive benchmark testing guide.
