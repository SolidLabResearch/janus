# Hybrid Coordination Benchmark Sustained Window Execution Design

Date: 2026-06-10

## Status

Accepted

## Context

The Hybrid Coordination Benchmark sustained-window path evaluates sustained query execution over continuous sliding windows. Unlike the first-result hybrid run, the sustained benchmark must measure latency and data transfer over repeated windows during virtual event-time streaming.

## Decision

1. Treat live-duration-seconds as logical event-time duration. Output logical_live_duration_seconds.
2. Record wall_clock_benchmark_duration_ms and uses_virtual_event_time: true.
3. Represent windows using canonical bounds: window_start_ms, window_end_ms, and window_id: window_start_ms + "-" + window_end_ms. Use these bounds for per-window hashing and equivalence checks.
4. Introduce a 2ms sleep during event addition to resolve the race condition with asynchronous window evaluation inside the streaming processor, ensuring deterministic window collection.
5. Introduce a sentinel watermark event at the end of duration (last timestamp + 20 seconds) to deterministically trigger finalization of active windows.
6. Exclude out-of-horizon and sentinel-finalized flush windows from summary statistics (latency, result count, and data transfer size) by default. Add completed_windows_total, completed_windows_in_horizon, and flush_windows metrics, and perform validation checks strictly on the horizon windows.

## Alternatives Considered

- Wall-clock time execution: Rejected because running the benchmark in real-time is slow, non-deterministic, and prone to CPU scheduling jitter. Virtual event-time streaming is fast and reproducible.
- Single timestamp window keys: Rejected because window start times alone do not represent window bounds. Combining start and end timestamps into a canonical string key window_id allows precise equivalence validation across sliding windows.
- Polling with long sleeps: Rejected because long sleeps slow down the benchmark. A minimal 2ms sleep after each event addition provides enough context-yield time for the background streaming processor thread to emit results without significantly impacting wall-clock duration.
