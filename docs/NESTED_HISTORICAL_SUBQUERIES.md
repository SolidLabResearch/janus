# Nested Historical Subqueries

This document explains the current Janus-QL support for historical materialized subqueries, the planning pipeline, and the small benchmark used to compare them with explicit `DEFINE BASELINE` syntax.

## What A Nested Historical Subquery Is

A nested historical subquery is a `SELECT { ... }` block inside the main live query `WHERE` clause whose input windows are historical `ON LOG` windows.

Current supported intent:

- evaluate the nested historical query once for its historical input
- materialize the result into an internal named graph
- rewrite the outer live query to join against that materialized result

This is a user-facing query shape whose runtime artifact is an internal materialized historical result. It is not a new end-user execution mode yet.

## Planning Pipeline

Current nested historical subqueries follow this pipeline:

```text
Parse
→ window dependency analysis
→ logical subquery plan
→ physical subquery plan
→ historical materialization
→ live query execution
```

In implementation terms:

1. `JanusQLParser::parse_ast()` parses the Janus-QL surface syntax.
2. Window dependency analysis classifies nested subquery window references as historical or live.
3. The planner produces a `LogicalSubqueryPlan`.
4. The planner maps that into a `PhysicalSubqueryPlan`.
5. `MaterializeHistoricalResult` lowers the historical materialized subquery into a materialized historical result plus a rewritten graph join in the outer query.
6. `JanusApi::start_query()` initializes the generated historical materialization and starts live execution.

The main benefit is separation of concerns:

- query syntax stays in the parser
- execution intent stays in logical planning
- concrete supported runtime behavior stays in physical planning and execution

## Supported Query Shapes

### Single Historical Window

```sparql
PREFIX ex: <http://example.org/>

FROM NAMED WINDOW ex:liveMinute ON STREAM ex:stream [RANGE 60000 STEP 1000]
FROM NAMED WINDOW ex:historyDay ON LOG ex:stream [START 0 END 86400000]

REGISTER RStream ex:output AS
SELECT ?sensor
       (AVG(?value) AS ?minuteAvgValue)
       ?dayAvgValue
WHERE {
  WINDOW ex:liveMinute {
    ?sensor ex:temperature ?value .
  }
  {
    SELECT ?sensor
           (AVG(?histValue) AS ?dayAvgValue)
    WHERE {
      WINDOW ex:historyDay {
        ?sensor ex:temperature ?histValue .
      }
    }
    GROUP BY ?sensor
  }
}
GROUP BY ?sensor ?dayAvgValue
```

Planner result:

- execution mode: `HistoricalMaterializedOnce`
- logical plan: `HistoricalMaterialized`
- physical plan: `MaterializeHistoricalResult`

### Multiple Historical Windows

```sparql
PREFIX ex: <http://example.org/>

FROM NAMED WINDOW ex:liveMinute ON STREAM ex:stream [RANGE 60000 STEP 1000]
FROM NAMED WINDOW ex:historyDay ON LOG ex:stream [START 0 END 86400000]
FROM NAMED WINDOW ex:historyWeek ON LOG ex:stream [START 0 END 604800000]

REGISTER RStream ex:output AS
SELECT ?sensor ?dayAvgValue
WHERE {
  WINDOW ex:liveMinute {
    ?sensor ex:temperature ?value .
  }
  {
    SELECT ?sensor
           (AVG(?histValue) AS ?dayAvgValue)
    WHERE {
      WINDOW ex:historyDay {
        ?sensor ex:temperature ?histValue .
      }
      WINDOW ex:historyWeek {
        ?sensor ex:temperature ?histValue .
      }
    }
    GROUP BY ?sensor
  }
}
```

Current behavior:

- both historical windows are tracked as dependencies
- the logical plan is still `HistoricalMaterialized`
- the physical plan is still `MaterializeHistoricalResult`
- execution loads both historical windows into synthetic named graphs keyed by window name

## Unsupported Query Shapes

### Mixed Live And Historical Nested Subquery

```sparql
PREFIX ex: <http://example.org/>

FROM NAMED WINDOW ex:liveMinute ON STREAM ex:stream [RANGE 60000 STEP 1000]
FROM NAMED WINDOW ex:historyDay ON LOG ex:stream [START 0 END 86400000]

REGISTER RStream ex:output AS
SELECT ?sensor ?liveValue ?histAvg
WHERE {
  {
    SELECT ?sensor ?liveValue (AVG(?histValue) AS ?histAvg)
    WHERE {
      WINDOW ex:liveMinute {
        ?sensor ex:temperature ?liveValue .
      }
      WINDOW ex:historyDay {
        ?sensor ex:temperature ?histValue .
      }
    }
    GROUP BY ?sensor ?liveValue
  }
}
```

This is classified as:

- execution mode: `LiveHistoricalJoin`
- logical plan: `LiveHistoricalJoin`
- physical plan: `Unsupported`

Current result:

- registration is rejected with a clean planning error

### Live-Only Nested Subquery

A nested subquery that references only live windows is also rejected for now.

This is classified as:

- execution mode: `LiveOnly`
- logical plan: `LiveSubquery`
- physical plan: `Unsupported`

### Subquery Without Any Known Window Reference

A nested subquery that does not reference a known `WINDOW` block is rejected as unsupported.

## Planner Diagnostics

Nested subquery planning emits structured diagnostics through `ParsedJanusQuery`:

- `planned_subqueries`
- `subquery_planning_diagnostics`
- `planning_statistics`

Example diagnostic summary:

```text
Nested subquery #0
Execution mode: HistoricalMaterializedOnce

Logical plan:
HistoricalMaterialized { windows: [...] }

Physical plan:
MaterializeHistoricalResult
```

The current statistics payload is:

```rust
struct QueryPlanningStatistics {
    historical_materialized_subqueries: usize,
    live_subqueries: usize,
    live_historical_joins: usize,
}
```

## How This Differs From Explicit DEFINE BASELINE

Explicit `DEFINE BASELINE` is user-visible syntax:

```sparql
DEFINE BASELINE ex:dayBaseline ON WINDOW ex:historyDay AS
SELECT ?sensor
       (AVG(?value) AS ?dayAvgValue)
WHERE {
  ?sensor ex:temperature ?value .
}
GROUP BY ?sensor
```

Nested historical subqueries are not exposed as named user baselines. They are:

- planned from a nested query block
- lowered into an internal materialization name
- joined back into the outer query automatically

Key difference:

- explicit `DEFINE BASELINE` lets the user name and reuse the baseline explicitly
- historical materialized subqueries keep the materialization internal to the planner and lowering path

## Why Janus Uses Internal Historical Materialized Subqueries

The nested form intentionally avoids introducing new user-visible baseline objects for a query that is written as one expression.

Reasons:

- the user wrote a nested query, not a reusable named artifact
- the planner can choose an execution strategy without changing query syntax
- future execution modes can reuse the same logical plan shape
- parser logic does not need to grow new execution-specific branches

In short:

- `DEFINE BASELINE` is the baseline syntax path
- historical materialized subqueries are a planner-owned lowering strategy that materializes an internal historical result

## Future Placeholder

The logical planner already includes a `LiveHistoricalJoin` stub for future work.

Expected future execution shape:

```text
Live window updates
       ↓
Historical lookup
       ↓
Join
       ↓
Continuous result updates
```

No such execution path is implemented yet.

## Benchmark

A small developer benchmark compares:

1. explicit `DEFINE BASELINE`
2. equivalent nested historical subquery

Runner:

```bash
cargo run --bin historical_materialized_subquery_benchmark -- --runs 3
```

The runner reports:

- parse time
- approximate planning/lowering time
- historical materialization time
- live startup time
- first result latency when observable

For a checked-in sample, see [NESTED_HISTORICAL_SUBQUERY_BENCHMARK_SAMPLE.md](./NESTED_HISTORICAL_SUBQUERY_BENCHMARK_SAMPLE.md).
