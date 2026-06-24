# Refactoring of Janus API for Modularity and Readability

Date: 2026-06-24

## Status

Accepted

## Context

The current implementation of the Janus high-level API is contained within a single file: [janus_api.rs](file:///Users/kushbisen/Code/janus/src/api/janus_api.rs). This file spans over 2600 lines and mixes several distinct responsibilities:
1. Public API type definitions and handles such as [QueryResult](file:///Users/kushbisen/Code/janus/src/api/janus_api/types.rs), [QueryHandle](file:///Users/kushbisen/Code/janus/src/api/janus_api/types.rs), and [RunningQuery](file:///Users/kushbisen/Code/janus/src/api/janus_api/types.rs).
2. The core [JanusApi](file:///Users/kushbisen/Code/janus/src/api/janus_api/core.rs) struct implementing query registration, startup, and stop.
3. Query-defined baseline engine execution, statements collection, accumulation, and materialization.
4. Validation logic for baseline definitions, step alignment, and templates.
5. RDF term parsing, unescaping, and normalization (including custom RDF literal helpers).
6. MQTT URI parsing logic via [parse_mqtt_uri](file:///Users/kushbisen/Code/janus/src/api/janus_api/mqtt.rs).

Having all these features combined in a single file makes the code harder to read, maintain, and test.

## Decision

1. Structure the API into a module directory [janus_api/](file:///Users/kushbisen/Code/janus/src/api/janus_api/) with:
   - [mod.rs](file:///Users/kushbisen/Code/janus/src/api/janus_api/mod.rs): Module declarations and re-exports of public APIs to maintain backward compatibility.
   - [types.rs](file:///Users/kushbisen/Code/janus/src/api/janus_api/types.rs): Definitions for [QueryResult](file:///Users/kushbisen/Code/janus/src/api/janus_api/types.rs), [QueryHandle](file:///Users/kushbisen/Code/janus/src/api/janus_api/types.rs), [RunningQuery](file:///Users/kushbisen/Code/janus/src/api/janus_api/types.rs), [ExecutionStatus](file:///Users/kushbisen/Code/janus/src/api/janus_api/types.rs), and [JanusApiError](file:///Users/kushbisen/Code/janus/src/api/janus_api/types.rs).
   - [core.rs](file:///Users/kushbisen/Code/janus/src/api/janus_api/core.rs): Core [JanusApi](file:///Users/kushbisen/Code/janus/src/api/janus_api/core.rs) struct and its implementation block.
   - [baseline.rs](file:///Users/kushbisen/Code/janus/src/api/janus_api/baseline.rs): Query-defined baseline evaluation, statements collection, and materialization helper functions.
   - [validation.rs](file:///Users/kushbisen/Code/janus/src/api/janus_api/validation.rs): Baseline definition validation, step alignment check, and graph template validation functions.
   - [rdf.rs](file:///Users/kushbisen/Code/janus/src/api/janus_api/rdf.rs): RDF term parsing, unescaping, normalization, and graph template term resolution.
   - [mqtt.rs](file:///Users/kushbisen/Code/janus/src/api/janus_api/mqtt.rs): The [parse_mqtt_uri](file:///Users/kushbisen/Code/janus/src/api/janus_api/mqtt.rs) utility function.
   - [tests.rs](file:///Users/kushbisen/Code/janus/src/api/janus_api/tests.rs): Unit and integration tests migrated from the original [janus_api.rs](file:///Users/kushbisen/Code/janus/src/api/janus_api.rs).
2. Re-export all public types and core structs in [mod.rs](file:///Users/kushbisen/Code/janus/src/api/janus_api/mod.rs) so that external imports do not break.

## Alternatives Considered

- Keep all implementation in a single file: Rejected because the file size is too large and hinders clean separation of concerns.
- Expose submodules directly to external users: Rejected to preserve public API compatibility. [mod.rs](file:///Users/kushbisen/Code/janus/src/api/janus_api/mod.rs) will keep all existing public APIs intact.
