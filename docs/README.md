# Janus Documentation

This directory contains the current Janus documentation.

## Start Here

- [DOCUMENTATION_INDEX.md](./DOCUMENTATION_INDEX.md): canonical reading order
- [JANUSQL.md](./JANUSQL.md): Janus-QL syntax and semantics
- [QUERY_EXECUTION.md](./QUERY_EXECUTION.md): how registration, startup, historical execution, live execution, and result delivery work
- [BASELINES.md](./BASELINES.md): implementation compatibility boundary for baseline-oriented paths
- [HTTP_API_CURRENT.md](./HTTP_API_CURRENT.md): current REST and WebSocket API
- [README_HTTP_API.md](./README_HTTP_API.md): compatibility link to the current HTTP API guide
- [QUICKSTART_HTTP_API.md](./QUICKSTART_HTTP_API.md): shortest API validation flow
- [ANOMALY_DETECTION.md](./ANOMALY_DETECTION.md): recommended anomaly-detection patterns and limitations
- [QUICK_REFERENCE.md](./QUICK_REFERENCE.md): short operational commands and endpoint summary

## Supporting Material

- [BENCHMARK_RESULTS.md](./BENCHMARK_RESULTS.md): retained historical benchmark summary
- [BENCHMARKING.md](./BENCHMARKING.md): current benchmark harness and run commands
- [PAPER_BENCHMARKING.md](./PAPER_BENCHMARKING.md): paper-facing benchmark harnesses and destinations
- [PAPER_ARTIFACT_MAP.md](./PAPER_ARTIFACT_MAP.md): paper claim to artifact map
- [PAPER_ARCHITECTURE.md](./PAPER_ARCHITECTURE.md): Mermaid architecture diagrams for the paper
- [PAPER_SUBMISSION_PACKAGE.md](./PAPER_SUBMISSION_PACKAGE.md): include/exclude list for a submission bundle
- [STREAM_BUS_CLI.md](./STREAM_BUS_CLI.md): replay and ingestion CLI

## Repo Boundary

This repository is the Janus backend and engine implementation.

The maintained dashboard lives in:

- `https://github.com/SolidLabResearch/janus-dashboard`

The static demo in this repository is only for local manual backend testing.
