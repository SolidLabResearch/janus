# Hybrid Scaling Query Types Benchmark Design

Date: 2026-06-17

## Status

Accepted

## Context

The hybrid scaling combined benchmark currently runs with a static queried historical window size of 1,000 events. To evaluate different scaling behaviors under various selectivity patterns inside the hybrid live-plus-historical query, the benchmark needs to support multiple query/access patterns: point_lookup, fixed_60s, range_10_percent, range_50_percent, and range_100_percent.

## Decision

1. Extend the CLI parser to support a `--historical-query-types` flag, accepting comma-separated strings of the target types.
2. Store sequential, unique quads in the historical SegmentedStorage to prevent deduplication and ensure that the query selectivity matches the physical range size.
3. Use a slight fractional offset in the object field (`40.0 + (index % 17) + index * 0.000001`) to guarantee quad uniqueness while keeping the baseline flow value within range to successfully join and match the live events.
4. Modify the decomposed baseline's `join_live_with_baseline_with_filter` to parse flow values as `f64` instead of `i32` to correctly handle fractional baseline averages.
5. Delineate and output the detailed query metrics: `historical_query_start_ms`, `historical_query_end_ms`, `historical_query_span_ms`, `historical_result_count`, `target_historical_quads`, and `historical_query_type` in `HybridScalingRow`, renaming `window_processing_overhead_ms` to `post_trigger_result_observation_delay_ms` for correctness.

## Alternatives Considered

- **Using distinct subjects for all events**: Rejected because only junctions with IRIs in `junction/{0..63}` exist in the live stream. If the query window selects subjects outside of this range (e.g. older history in larger stores), the hybrid query would have a zero result count, complicating verification of correct end-to-end execution.
- **Using unique predicates**: Rejected because the query's graph patterns are written with a static predicate `ex:baselineFlow`. Changing the predicate would require rewriting the query patterns dynamically, introducing parsing/registration noise.
