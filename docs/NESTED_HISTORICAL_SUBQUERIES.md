# Nested Historical Subqueries

Janus supports a restricted nested `SELECT` pattern for materializing
historical bindings that a hybrid live query can use.

## Supported shape

The nested query must operate on declared `ON LOG` window(s) and produce the
bindings needed by the outer query. A common use is a historical aggregate per
sensor combined with a live value in the outer query. See the complete example
in [Janus-QL](./JANUSQL.md#hybrid-and-nested-historical-queries).

## Planning

1. The parser validates named windows and recognizes the nested historical
   `SELECT`.
2. Janus lowers the historical subquery to a storage-backed SPARQL execution.
3. The resulting bindings are materialized for the supported live execution
   path.
4. The outer query receives live results when its stream window closes.

## Rejected shapes

- a live-only nested subquery;
- a nested subquery that mixes live and historical windows;
- an unsupported projection or materialization shape; or
- references to undeclared windows.

The benchmark binary `historical_materialized_subquery_benchmark` exercises
this path. Its output is benchmark evidence only when accompanied by the
command, revision, environment, and raw result files.
