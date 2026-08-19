# Historical Materialization Benchmark

The legacy query-defined-baseline benchmark is retained for implementation
compatibility. For current public Janus-QL work, use the nested historical
materialization path and its executable benchmark:

```bash
cargo run --release --bin historical_materialized_subquery_benchmark -- --help
```

Record both the query fixture and its lowering/correctness evidence. A timing
row alone does not demonstrate that two query forms are semantically
equivalent. See [Nested Historical Subqueries](./NESTED_HISTORICAL_SUBQUERIES.md)
and [Benchmarking](./BENCHMARKING.md).
