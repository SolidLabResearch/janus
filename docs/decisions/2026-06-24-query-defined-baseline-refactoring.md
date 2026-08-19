# Refactoring of Query Defined Baseline Benchmark for Readability

Date: 2026-06-24

## Status

Accepted

> Historical compatibility record. It does not expand the public Janus-QL
> contract; see `docs/JANUSQL.md` for that contract.

## Context

The benchmark implementation in src/paper_bench/query_defined_baseline.rs is currently a single large file spanning over 2400 lines. It mixes distinct responsibilities:
1. Benchmark configuration and result reporting models.
2. Low-level RDF term parsing, template validation, and SPARQL binding materialization.
3. System resource sampling (CPU and memory usage tracking).
4. Storage generation, prep, and writing helper functions.
5. Baseline and live-only benchmark runners.
6. Diagnostic collection and validation assertions.

The file is difficult to read, navigate, and maintain. Developers looking to understand or modify the benchmark have to filter through unrelated parsing details, metric reporting layouts, and system metrics collection code.

## Decision

Structure src/paper_bench/query_defined_baseline.rs into a module directory (src/paper_bench/query_defined_baseline/) with the following components:
- mod.rs: Main entry point for the benchmark execution runner, exposing all public configurations, tests, and types for backward compatibility.
- types.rs: Model definitions for metrics, rows, configurations, and outcomes.
- storage.rs: Storage initialization, historical data writing, and simulated event generators.
- system.rs: Resource metrics collector implementation.
- runner.rs: Core query processing variant execution (baseline vs live-only comparison flow).
- validation.rs: Observed row parsing, window summarization, correctness evaluation, and validation diagnostics.
- rdf.rs: Utility parser for RDF literals, term resolution, and SPARQL binding quad materialization.
- reporting.rs: Statistics computation, CSV writing, markdown table formatting, and JSON output serializer.

This split isolates modules by concern. It simplifies parsing/formatting details while keeping the main orchestrator flow easy to trace.

## Alternatives Considered

- Keep all code in a single file and refactor using multiple impl blocks: Rejected because the file length would still exceed 2000 lines, which does not address the readability and maintenance challenges of navigating a huge single file.
- Move generic RDF helpers to a common library utility module: Rejected for now because these helpers are tailored to the parsing of query defined template variables and string-binding layouts. A local rdf submodule keeps the focus on this benchmark's specific needs without changing other core library directories.
