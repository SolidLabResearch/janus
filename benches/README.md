# RDF Indexing Benchmarks

This directory contains comprehensive benchmarks for comparing different RDF indexing strategies in Janus.

## Available Benchmarks

### 1. `benchmark.rs` - Read Performance (Original)
Tests query performance on pre-built indexes:
- Index building time comparison
- Query speed across different data ranges
- Memory usage comparison

### 2. `write_benchmark.rs` - Write Performance (New)
Tests writing performance during record insertion:
- Real-time indexing (index while writing)
- Batch indexing (build index after writing)
- Writing throughput comparison
- Total processing time analysis

### 3. `analysis.rs` - Advanced Analysis (New)
Detailed analysis across multiple dimensions:
- Optimal sparse interval analysis
- Memory usage scaling
- Write throughput under different conditions
- Performance recommendations

## Quick Start

### Run All Benchmarks
```bash
./run_benchmarks.sh
```

### Run Individual Benchmarks
```bash
# Original read performance benchmark
cargo bench --bench benchmark

# New write performance benchmark  
cargo bench --bench write_benchmark

# Advanced analysis suite
cargo bench --bench analysis
```

## Step-by-Step Testing Instructions

Benchmarks are organized by functionality:

- `query_parsing.rs` - Benchmarks for parsing RSP-QL queries
- `stream_processing.rs` - Benchmarks for stream processing operations
- `store_operations.rs` - Benchmarks for RDF store interactions
- `integration.rs` - End-to-end integration benchmarks

## Adding New Benchmarks

To add a new benchmark:

1. Create a new file in the `benches/` directory
2. Add the benchmark to `Cargo.toml`:

```toml
[[bench]]
name = "my_benchmark"
harness = false
```

3. Use the `criterion` crate for benchmarking:

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_function(c: &mut Criterion) {
    c.bench_function("my_function", |b| {
        b.iter(|| {
            // Code to benchmark
            black_box(my_function())
        });
    });
}

criterion_group!(benches, benchmark_function);
criterion_main!(benches);
```

## Benchmark Results

Benchmark results are stored in `target/criterion/` and include:

- HTML reports with graphs
- Comparison with previous runs
- Statistical analysis

To view results, open `target/criterion/report/index.html` in a browser.

## Performance Tips

- Run benchmarks in release mode (default for `cargo bench`)
- Ensure system is idle during benchmarking
- Use consistent hardware for comparisons
- Run multiple iterations to reduce noise
- Use `black_box()` to prevent compiler optimizations