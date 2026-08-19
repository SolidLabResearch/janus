# Refactoring of Paper Bench Harness for Modularity and Readability

Date: 2026-06-24

## Status

Accepted

> Historical architecture record. Use `src/paper_bench/harness/` and
> `docs/PAPER_BENCHMARKING.md` for the current layout and commands.

## Context

The paper_bench harness (src/paper_bench/harness.rs) currently contains all benchmark execution flows, system configuration detection, data generation helpers, logging/CSV export, and statistics gathering inside a single file exceeding 3200 lines of code. This size makes understanding and maintaining the benchmark harness difficult.

## Decision

1. Extract the single harness.rs into a directory-based module src/paper_bench/harness/ containing logical submodules:
   - types.rs: Benchmark structures and configuration types.
   - system_info.rs: System hardware, OS, and memory information capture.
   - data_gen.rs: Synthetic dataset generators (such as CityBench) and event sequence builders.
   - io.rs: File writing, CSV reports, JSONL logs, and debug artifact exporters.
   - metrics.rs: Percentile, mean, linear regression fit, and run summary calculations.
   - coordination.rs: H1.1 Hybrid Coordination benchmarking logic.
   - sustained.rs: H1.2 Sustained Hybrid window execution benchmarking logic.
   - scaling.rs: H2 Historical Scaling queries and point/range lookup benchmarks.
   - helpers.rs: Normalization, canonical hashing, RSP-QL parsing, time tracking, and event scheduling helpers.
2. In src/paper_bench/harness/mod.rs, re-export all public symbols from the submodules to maintain backwards compatibility with existing benchmark binaries and test modules.
3. Move the unit tests from the single file to the relevant submodule files or mod.rs to ensure continued verification.

## Alternatives Considered

- Keep harness.rs as a single file and apply comment blocks to visually separate concerns. Rejected because the file size remains too large for readability and modern IDE navigation.
- Move components to top-level modules under src/paper_bench/. Rejected because this would break the public interface for the existing benchmark binaries that depend on the harness module structure.
