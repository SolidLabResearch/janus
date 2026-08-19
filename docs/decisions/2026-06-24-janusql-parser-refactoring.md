# Refactoring of JanusQL Parser for Modularity and Readability

Date: 2026-06-24

## Status

Proposed

> Historical architecture record. The refactoring has since landed; paths in
> the context describe the pre-refactor state. Use `src/parsing/janusql_parser/`
> and `docs/JANUSQL.md` for current work.

## Context

The current implementation of the JanusQL parser is contained within a single file, src/parsing/janusql_parser.rs, which is over 2200 lines long. This file handles multiple distinct tasks:
1. AST structures and definitions (e.g., JanusQueryAst, ParsedJanusQuery, WindowDefinition).
2. The core parsing driver.
3. Syntax parsing for SELECT, FROM NAMED WINDOW, WHERE, PREFIX, REGISTER, USING/DEFINE BASELINE.
4. Nested subquery analysis, classification, and planning.
5. WHERE clause processing, window body lookups, and adaptation.
6. Graph template parsing and extraction.
7. Utility functions such as matching braces, extracting variables, and unwrapping/wrapping IRIs.

This single large file makes maintenance, readability, and modification difficult.

## Decision

We will refactor src/parsing/janusql_parser.rs into a module directory (src/parsing/janusql_parser/) to split the concerns:
1. src/parsing/janusql_parser/mod.rs: The entry point, re-exporting the public AST and Parser structs. It will implement the primary driver methods: new, parse, and parse_ast.
2. src/parsing/janusql_parser/ast.rs: Definition of all AST structs and enums (e.g., WindowType, WindowDefinition, WindowSpec, JanusQueryAst, ParsedJanusQuery).
3. src/parsing/janusql_parser/subquery.rs: Logic for subquery planning, lowered execution mode classification, statistics tracking, and nested subquery lowering.
4. src/parsing/janusql_parser/where_clause.rs: Logic for WHERE clause adaptation, window body search, and extracting where window clauses.
5. src/parsing/janusql_parser/graph.rs: Graph template splitting, statement tokenization, graph term template parsing, and GRAPH block parsing.
6. src/parsing/janusql_parser/clauses.rs: Parsers for REGISTER, BASELINE, PREFIX, WINDOW, and subquery SELECT/WHERE components.
7. src/parsing/janusql_parser/generation.rs: Generation of output queries (RSPQL, SPARQL, and baseline SPARQL queries).
8. src/parsing/janusql_parser/utils.rs: Low-level utilities (brace matching, IRI wrapping/unwrapping, variables/projection extraction).

All public structures and their public interfaces will be re-exported in mod.rs to ensure zero disruption to current consumers.

## Alternatives Considered

- Refactoring within the single file using separate impl blocks: Rejected because the file size would remain massive and hard to navigate.
- Splitting into modules but leaving public structures in individual files without re-exporting: Rejected because it would break existing code and test suites. Public API compatibility must be maintained.
