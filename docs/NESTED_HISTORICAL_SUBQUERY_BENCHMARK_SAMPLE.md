# Nested Historical Subquery Benchmark Sample

Example command:

```bash
cargo run --bin historical_materialized_subquery_benchmark -- \
  --runs 2 \
  --historical-events 5000 \
  --entity-count 5
```

Example output:

```text
benchmark=nested_historical_subquery
historical_events=5000
entity_count=5
runs=2

query=explicit_define_baseline
  parse_ast_ms_avg=0.421
  parse_total_ms_avg=0.901
  planning_lowering_ms_avg=0.480
  register_ms_avg=0.936
  historical_materialization_ms_avg=3.112
  live_startup_ms_avg=4.281
  baseline_bindings=5
  first_result_latency_ms_avg=n/a

query=nested_historical_subquery
  parse_ast_ms_avg=0.394
  parse_total_ms_avg=0.882
  planning_lowering_ms_avg=0.488
  register_ms_avg=0.919
  historical_materialization_ms_avg=3.205
  live_startup_ms_avg=4.337
  baseline_bindings=5
  first_result_latency_ms_avg=n/a

delta_nested_minus_explicit
  parse_total_ms_avg=-0.019
  planning_lowering_ms_avg=0.008
  historical_materialization_ms_avg=0.093
  live_startup_ms_avg=0.056
```

Notes:

- `planning_lowering_ms_avg` is computed as `parse_total_ms_avg - parse_ast_ms_avg`
- `first_result_latency_ms_avg` is often unavailable in this microbenchmark because it does not inject live events
- the expected outcome is that both query forms stay close because they converge on the same historical materialization path
