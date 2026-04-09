# Janus Documentation

This directory contains the project documentation for Janus.

Some older files in this directory are design notes, implementation logs, or milestone-specific writeups. The files below are the current starting point for understanding how Janus works today.

## Start Here

- [DOCUMENTATION_INDEX.md](./DOCUMENTATION_INDEX.md): canonical reading order
- [JANUSQL.md](./JANUSQL.md): Janus-QL syntax and semantics
- [QUERY_EXECUTION.md](./QUERY_EXECUTION.md): how registration, startup, historical execution, live execution, and result delivery work
- [BASELINES.md](./BASELINES.md): `USING BASELINE`, `LAST`, `AGGREGATE`, and async warm-up
- [HTTP_API_CURRENT.md](./HTTP_API_CURRENT.md): current REST and WebSocket API
- [ANOMALY_DETECTION.md](./ANOMALY_DETECTION.md): recommended anomaly-detection patterns and limitations

## Supporting Material

- [ARCHITECTURE.md](./ARCHITECTURE.md): older high-level architecture notes
- [EXECUTION_ARCHITECTURE.md](./EXECUTION_ARCHITECTURE.md): historical execution design notes
- [HTTP_API.md](./HTTP_API.md): earlier HTTP API writeup
- [BENCHMARK_RESULTS.md](./BENCHMARK_RESULTS.md): benchmark data

## Notes

- The canonical docs above are intended to describe the current implementation on `main` once merged.
- Older files are still useful for background, but they may describe previous milestones or implementation states.
