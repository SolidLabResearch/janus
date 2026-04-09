# Janus Documentation

This directory contains the project documentation for Janus.

Some files here are current product documentation. Others are older design or
milestone notes kept only for background context.

## Start Here

- [DOCUMENTATION_INDEX.md](./DOCUMENTATION_INDEX.md): canonical reading order
- [JANUSQL.md](./JANUSQL.md): Janus-QL syntax and semantics
- [QUERY_EXECUTION.md](./QUERY_EXECUTION.md): how registration, startup, historical execution, live execution, and result delivery work
- [BASELINES.md](./BASELINES.md): `USING BASELINE`, `LAST`, `AGGREGATE`, and async warm-up
- [HTTP_API_CURRENT.md](./HTTP_API_CURRENT.md): current REST and WebSocket API
- [ANOMALY_DETECTION.md](./ANOMALY_DETECTION.md): recommended anomaly-detection patterns and limitations
- [QUICK_REFERENCE.md](./QUICK_REFERENCE.md): short operational commands and endpoint summary

## Supporting Material

- [ARCHITECTURE.md](./ARCHITECTURE.md): older high-level architecture notes
- [EXECUTION_ARCHITECTURE.md](./EXECUTION_ARCHITECTURE.md): historical execution design notes
- [BENCHMARK_RESULTS.md](./BENCHMARK_RESULTS.md): benchmark data

## Notes

- The files listed under Start Here are the current sources of truth for `main`.
- Frontend development does not happen in this repository. The maintained web
  dashboard lives in the separate `janus-dashboard` repository.
