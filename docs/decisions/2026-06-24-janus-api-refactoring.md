# Refactoring of Janus API for Modularity and Readability

Date: 2026-06-24

## Status

Accepted. This is a historical architecture decision; for current behavior,
use the executable modules and [the current HTTP API](../HTTP_API_CURRENT.md).

## Context

The Janus API had accumulated public types, lifecycle orchestration, baseline
compatibility, RDF helpers, and MQTT URI parsing in one source file. The code
was refactored into a module directory to make these concerns independently
testable.

## Decision

The API is organized under [src/api/janus_api/](../../src/api/janus_api/):

- [mod.rs](../../src/api/janus_api/mod.rs) exposes the public module surface.
- [types.rs](../../src/api/janus_api/types.rs) defines API result and lifecycle
  types.
- [core.rs](../../src/api/janus_api/core.rs) implements registration, startup,
  and stopping.
- [baseline.rs](../../src/api/janus_api/baseline.rs) and
  [validation.rs](../../src/api/janus_api/validation.rs) retain
  baseline-oriented compatibility behavior.
- [rdf.rs](../../src/api/janus_api/rdf.rs) contains RDF term helpers.
- [mqtt.rs](../../src/api/janus_api/mqtt.rs) parses MQTT URIs.
- [tests.rs](../../src/api/janus_api/tests.rs) contains focused API tests.

The modules continue to be exposed through `mod.rs` rather than requiring
consumers to depend on the internal file layout.

## Consequence

This decision records the refactoring rationale, not a public Janus-QL syntax
commitment. Refer to [JANUSQL.md](../JANUSQL.md) for the current public query
surface.
