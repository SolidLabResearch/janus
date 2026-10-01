# Standalone Janus-QL parser boundary

## Decision

Janus delegates Core Janus-QL syntax parsing, validation, and normalization to
the standalone `janusql-parser` crate. `JanusQLParser` is retained as a
compatibility facade and lowering entry point. `ParsedJanusQuery` is an engine
representation, not a syntax AST.

## Boundary

`DEFINE BASELINE` and `USING BASELINE` are Janus-only compatibility clauses.
Janus removes only those clauses before handing unchanged Core source to the
standalone parser. Janus then lowers the typed Core AST for generation,
materialization, and execution.
