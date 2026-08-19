# Refactoring of HTTP Server module for Modularity and Readability

Date: 2026-06-24

## Status

Accepted

> Historical architecture record. The current module layout and API contract
> are documented in `src/http/` and `docs/HTTP_API_CURRENT.md`.

## Context

The current implementation of the Janus HTTP server is contained within a single file, src/http/server.rs, which is over 800 lines long. This file handles multiple distinct concerns:
1. Endpoint DTO structures (requests and responses).
2. Shared application state and replay state.
3. Custom API error definitions and Axum response serialization.
4. Route handlers for query execution, websocket streaming, stream bus replay, and health checks.

This single massive file compromises readability, maintainability, and testing isolation.

## Decision

We will refactor src/http/server.rs into a modular package structure:
1. src/http/error.rs: Move `ApiError` and `ErrorResponse`, and implement formatting/response traits.
2. src/http/types.rs: Move request/response DTO structs, standard default serde helpers, `AppState`, `QueryResultBroadcast`, and `ReplayState`.
3. src/http/handlers/mod.rs: Re-export endpoint handlers.
4. src/http/handlers/query.rs: Implement query-related handlers and their unit tests.
5. src/http/handlers/replay.rs: Implement replay-related handlers.
6. src/http/handlers/status.rs: Implement health check and ops status handlers.

We will modify src/http/mod.rs and src/http/server.rs to declare and re-export the new sub-modules, preserving exact API compatibility.

## Alternatives Considered

- Refactoring within the single file using separate impl blocks: Rejected because the file size would remain massive and hard to navigate.
- Splitting into modules but leaving public structures in individual files without re-exporting: Rejected because it would break existing code and test suites. Public API compatibility must be maintained.
