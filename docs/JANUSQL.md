# Janus-QL

Janus-QL is the query language Janus uses to describe historical windows, live windows, and hybrid queries.

## Query Shape

A Janus-QL query typically contains:

- `PREFIX` declarations
- a `REGISTER` clause
- one or more `FROM NAMED WINDOW` clauses
- an optional `USING BASELINE` clause
- optional `DEFINE BASELINE ... ON WINDOW ... AS SELECT ...` clauses
- a `WHERE` clause with `WINDOW <name> { ... }` blocks

Example:

```sparql
PREFIX ex: <http://example.org/>
PREFIX janus: <https://janus.rs/fn#>
PREFIX baseline: <https://janus.rs/baseline#>

REGISTER RStream ex:out AS
SELECT ?sensor ?reading
FROM NAMED WINDOW ex:hist ON LOG ex:store [START 1700000000000 END 1700003600000]
FROM NAMED WINDOW ex:live ON STREAM ex:stream1 [RANGE 5000 STEP 1000]
USING BASELINE ex:hist AGGREGATE
WHERE {
  WINDOW ex:hist {
    ?sensor ex:mean ?mean .
    ?sensor ex:sigma ?sigma .
  }
  WINDOW ex:live {
    ?sensor ex:hasReading ?reading .
  }
  ?sensor baseline:mean ?mean .
  ?sensor baseline:sigma ?sigma .
  FILTER(janus:is_outlier(?reading, ?mean, ?sigma, 3))
}
```

## Supported Window Types

### Live Sliding Window

Use `ON STREAM` with `RANGE` and `STEP`.

```sparql
FROM NAMED WINDOW ex:live ON STREAM ex:stream1 [RANGE 5000 STEP 1000]
```

This becomes part of the generated RSP-QL query.

### Historical Fixed Window

Use `ON LOG` with `START` and `END`.

```sparql
FROM NAMED WINDOW ex:hist ON LOG ex:store [START 1700000000000 END 1700003600000]
```

This becomes a one-shot historical SPARQL execution over storage.

### Historical Sliding Window

Use `ON LOG` with `OFFSET`, `RANGE`, and `STEP`.

```sparql
FROM NAMED WINDOW ex:hist ON LOG ex:store [OFFSET 3600000 RANGE 300000 STEP 300000]
```

At live evaluation time `T`, this resolves to:

```text
[T - OFFSET - RANGE, T - OFFSET]
```

Example:

```text
T = 172800000
OFFSET = 86400000
RANGE = 60000

=> [86340000, 86400000]
```

## Baseline Clause

Janus supports an optional clause:

```sparql
USING BASELINE ex:hist LAST
```

or:

```sparql
USING BASELINE ex:hist AGGREGATE
```

Semantics:

- the clause must reference a historical window
- that historical window is used to bootstrap baseline values for the live query
- `LAST` and `AGGREGATE` control how historical sliding-window results are collapsed before they are exposed to live evaluation

If the clause is absent, the HTTP/API registration-level `baseline_mode` is used as a fallback.

## Query-Defined Baselines

Janus also supports query-defined baselines. They are evaluated over a historical `LOG` window and
stored as named `SELECT`-result snapshots. During live evaluation, Janus resolves the snapshot
that matches the current evaluation time and joins it into the live query.

Canonical example:

```sparql
PREFIX : <http://example.org/>

FROM NAMED WINDOW :liveMinute ON STREAM :stream [RANGE 60000 STEP 1000]
FROM NAMED WINDOW :historyDay ON LOG :stream [START 0 END 86400000]

DEFINE BASELINE :dayBaseline ON WINDOW :historyDay AS
SELECT ?sensor
       (AVG(?value) AS ?dayAvgValue)
WHERE {
  ?sensor :hasValue ?value .
}
GROUP BY ?sensor

REGISTER RStream :output AS
USING BASELINE :dayBaseline
SELECT ?sensor
       (AVG(?value) AS ?minuteAvgValue)
       ?dayAvgValue
       ((AVG(?value) - ?dayAvgValue) AS ?difference)
WHERE {
  WINDOW :liveMinute {
    ?sensor :hasValue ?value .
  }

  GRAPH :dayBaseline {
    ?sensor :dayAvgValue ?dayAvgValue .
  }
}
GROUP BY ?sensor ?dayAvgValue
HAVING(AVG(?value) > ?dayAvgValue)
```

Semantics:

- `DEFINE BASELINE :dayBaseline ON WINDOW :historyDay AS SELECT ...` defines the historical
  binding relation used to build baseline snapshots.
- `USING BASELINE :dayBaseline` marks that baseline as required for live evaluation.
- `GRAPH :dayBaseline { ... }` remains the compatibility join shape for the live query.
- baseline variables such as `?dayAvgValue` are then available to the live query in `SELECT`, `GROUP BY`, `HAVING`, and arithmetic expressions.

## Query-Defined Baseline Materialization

Materialization is template-driven rather than inferred from `SELECT` aliases:

- the `GRAPH :baselineName { ... }` block defines the RDF shape that is inserted
- the baseline query must project every variable used by that template
- template predicates must be concrete IRIs
- template subjects must resolve to an IRI or blank node

Example binding:

- `?sensor = :s1`
- `?dayAvgValue = 42.0`

becomes:

```trig
GRAPH :dayBaseline {
  :s1 :dayAvgValue 42.0 .
}
```

## What Janus Generates Internally

The parser splits the query into:

- one live RSP-QL query built from live windows
- one SPARQL query per historical window
- one SPARQL query per query-defined baseline

Important detail:

- non-window patterns in the `WHERE` clause are preserved in the live query
- this is what makes joins such as `GRAPH :dayBaseline { ?sensor :dayAvgValue ?dayAvgValue . }`
  work during live execution

## Baseline Predicates

Legacy `USING BASELINE <window> LAST|AGGREGATE` values are exposed to the live side as static triples under:

```text
https://janus.rs/baseline#<variable_name>
```

So a historical binding:

- `?sensor = ex:s1`
- `?mean = 21.5`

becomes the static triple:

```text
ex:s1  <https://janus.rs/baseline#mean>  "21.5"
```

This is why live queries join on `baseline:*` predicates rather than directly reusing historical bindings.

Query-defined baselines use the baseline name as the graph IRI for the evaluation-local join view:

```trig
GRAPH :dayBaseline {
  :s1 :dayAvgValue 42.0 .
}
```

The live query must therefore join through `GRAPH :dayBaseline { ... }` rather than through `baseline:*` predicates.

## Limitations

- query-defined baselines support `SELECT` snapshots only
- `CONSTRUCT` baselines are not supported
- sliding historical baseline `STEP` must match the live `STEP`
- the `GRAPH` template predicates must be concrete IRIs
- template variables must be projected by the baseline query
- template subjects must resolve to an IRI or blank node
- the current implementation is tested for projected variables, `AVG`, `GROUP BY`, and arithmetic
  joins in the live query

## Practical Guidance

- Use fixed historical windows when you want one clean baseline snapshot.
- Use historical sliding windows only when you really need a baseline derived from multiple historical subwindows.
- Keep historical baseline queries compact. Prefer one row per anchor such as one row per sensor.
