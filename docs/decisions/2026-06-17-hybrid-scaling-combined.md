# Hybrid Scaling Combined Benchmark Design

Date: 2026-06-17

## Status

Accepted

## Context

Janus needs a combined hybrid historical and live query benchmark that scales the historical store size while running both Janus unified and decomposed baseline executions with the same deterministic live stream trace in realtime.

## Decision

1. Create a new binary hybrid_scaling_combined in src/bin/hybrid_scaling_combined.rs.
2. Keep the queried historical window static at 1,000 events, representing exactly 1,000 quads.
3. Scale the total store size H by writing H - 1,000 older events prior to the queried window. This keeps query bounds and query strings identical across all H, eliminating compiler and query plan variance while testing if index size affects bounded lookup performance.
4. Implement a realtime event replay pacing loop using event intervals to simulate realtime streaming accurately.
5. Record both target_historical_quads and actual_historical_quads.
6. Generate JSON, CSV, and Markdown reports.

## Alternatives Considered

- **Dynamic Query Bounds**: Growing the historical database forward and changing the query window start/end timestamps based on H. Rejected because this changes the query string on every size H, causing varying query registration overhead and caching behavior.
- **Accelerated Live Processing**: Running the live stream as fast as possible without sleeps. Rejected because the timing metrics (like first hybrid result delay relative to start and window overheads) require realtime pacing to match realistic RDF stream processing engines.
