# Janus-QL Grammar Audit

Date: 2026-07-09

This note compares the Janus-QL Core EBNF in `/Users/kushbisen/Code/janusql-spec/spec-src/sections/03a-core-grammar.bs` with the current implementation in `/Users/kushbisen/Code/janus/src/parsing/janusql_parser/`.

## Verdict

The EBNF is broadly faithful for the public Janus-QL Core window model:

- `PREFIX` declarations are parsed by `parse_prefix_declaration()`.
- `REGISTER RStream ... AS` is optional and enforced by `parse_register_clause()`.
- `FROM NAMED WINDOW` declarations are split into `ON STREAM [RANGE ... STEP ...]`, `ON LOG [START ... END]`, and `ON LOG [OFFSET ... RANGE ... STEP ...]` by `parse_window_clause()`.
- `WINDOW <name> { ... }` references are extracted from `WHERE` and checked against declared windows by `validate_where_window_references()`.
- Historical-only nested subqueries are identified in `extract_nested_subqueries()` and restricted during subquery planning in `plan_nested_subqueries()` and `validate_subquery_plan()`.
- Top-level `GROUP BY` and `HAVING` are preserved in the AST and in generated historical SPARQL.

The main mismatch is that the implementation is looser than the EBNF around `SELECT` items and `WINDOW` bodies:

- `SELECT` clauses are stored as text. The parser does not fully parse projection items against a strict `Var | (Expression AS Var)` grammar.
- `WINDOW` bodies are also stored as text. The parser does not fully parse triple patterns or `FILTER` expressions. It performs only shallow Janus-specific checks such as undeclared windows, `SERVICE` rejection, and property-path rejection.
- As a result, the EBNF is a conservative public contract, but the implementation may accept some extra SPARQL-like shapes as opaque body text.

## Syntax vs Semantic Validation

The current implementation splits enforcement across phases.

Rejected during parsing:

- `ON STREAM [START ... END]`
- `ON STREAM [OFFSET ... RANGE ... STEP ...]`
- `ON LOG [RANGE ... STEP ...]`
- unsupported `REGISTER` operators such as `IStream` and `DStream`

Rejected during later validation or planning:

- duplicate window names
- `RANGE == 0` or `STEP == 0`
- `START >= END`
- historical sliding `RANGE > OFFSET`
- undeclared `WINDOW` references
- nested subqueries with no known `WINDOW`
- nested subqueries that reference live windows only
- nested subqueries that mix live and historical windows

This distinction matters for the specification text: some invalid combinations are not merely semantic constraints over a permissive grammar; they are already rejected by the parser based on the `ON STREAM` versus `ON LOG` token sequence.

## Construct Coverage

The implementation and tests currently cover the requested constructs as follows:

- `PREFIX` declarations: implemented and tested.
- optional `REGISTER RStream`: implemented and tested, including historical-only queries without `REGISTER`.
- `SELECT` with variables and aliased expressions: implemented as text-preserving parsing with output-variable extraction; tested with variables and aggregate aliases.
- `FROM NAMED WINDOW`: implemented and tested.
- `ON STREAM` with `RANGE/STEP`: implemented and tested.
- `ON LOG` with fixed `START/END`: implemented and tested.
- `ON LOG` with sliding `OFFSET/RANGE/STEP`: implemented and tested.
- `WHERE` clauses with `WINDOW` blocks: implemented and tested.
- triple patterns: passed through inside `WINDOW` bodies; not structurally parsed by Janus.
- `FILTER` expressions: passed through inside `WINDOW` bodies; not structurally parsed beyond Janus-specific exclusions.
- historical nested `SELECT` subqueries: implemented and tested.
- `GROUP BY` and `HAVING`: implemented and tested for top-level historical queries and nested historical subqueries.

## SPARQL-Inherited Terminals

The specification says Janus-QL Core inherits SPARQL-style lexical forms for `IRIREF`, `PrefixedName`, `Literal`, `Name`, `PrefixName`, and `Var`.

That is directionally true, but the current implementation does not contain a full SPARQL lexer for those terminals. In practice:

- Janus parses the Janus-specific top-level clauses itself.
- prefixed names and `<iri>` forms are handled by lightweight token splitting and prefix expansion.
- triple-pattern terms and most expression text are forwarded as SPARQL-like text rather than fully validated by the Janus parser.

## Recommended Specification Changes

- Keep the current EBNF split between `ON STREAM` and `ON LOG`; it matches implementation behavior.
- Add an explicit note near the EBNF that `SELECT` items, triple patterns, and `FILTER` expressions are specified conservatively, while the current implementation uses shallow parsing and may accept extension syntax as opaque SPARQL-like text.
- In the invalid examples section, add explicit examples for:
  - `ON STREAM [START ... END]`
  - `ON STREAM [OFFSET ... RANGE ... STEP ...]`
  - `ON LOG [RANGE ... STEP ...]`
- Keep the nested-subquery rule outside the pure EBNF as a validation/planning rule: the syntax shape is parseable, but live-only and mixed nested subqueries are rejected semantically after window-dependency analysis.

## Local Test Updates

This repo now has explicit public-spec regression tests for:

- invalid `ON STREAM [START ... END]`
- invalid `ON STREAM [OFFSET ... RANGE ... STEP ...]`
- invalid `ON LOG [RANGE ... STEP ...]`

Those were added to `/Users/kushbisen/Code/janus/tests/public_spec_behavior_test.rs`.
