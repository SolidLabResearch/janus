# Benchmarks

This directory contains performance benchmarks for the Janus RDF Stream Processing Engine.

## Running Benchmarks

To run all benchmarks:

```bash
cargo bench
```

To run a specific benchmark:

```bash
cargo bench --bench <benchmark_name>
```

## Benchmark Structure

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