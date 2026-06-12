# H1.1 Benchmarking Metrics and Representation

Date: 2026-06-10

## Status

Accepted

## Context

The H1.1 benchmark validates Janus unified versus decomposed Oxigraph equivalence. To clarify the role of the live-streaming query component and coordinate join stages, the benchmark must clearly delineate the latencies of individual query steps (historical query, live-stream processing, and external join/combination).

## Decision

1. Rename the decomposed baseline to `"Oxigraph historical + Janus live window processor + external join"`.
2. Measure and report latencies:
   - `live_stream_processing_latency_ms`: Calculated as `first_result_engine - first_event_published` since live stream events are added back-to-back.
   - `external_join_latency_ms`: Set to `0.0` for JanusUnified. For Decomposed, calculated as the client-side join execution duration.
   - `first_hybrid_result_latency_ms`: Set to the total time from query registration/launch to client-side final output.
3. Compare final hybrid result correctness (`result_hash` and `result_count`) for the primary equivalence assertion, while reporting historical and live equivalence separately.

## Alternatives Considered

- **Zero-latency assumption for Janus hybrid combination**: Rejected because hybrid combination still occurs inline within the engine for Janus unified. An external client-side join duration is instead represented by `external_join_latency_ms`.
- **Excluding live intermediate result hash**: Accepted because `Janus Unified` executes the join inline and does not generate a separate live intermediate result. Live intermediate result equivalence is labeled as not applicable, but final hybrid and historical queries are verified.
