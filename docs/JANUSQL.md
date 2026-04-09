# Janus-QL

Janus-QL is the query language Janus uses to describe historical windows, live windows, and hybrid queries.

## Query Shape

A Janus-QL query typically contains:

- `PREFIX` declarations
- a `REGISTER` clause
- one or more `FROM NAMED WINDOW` clauses
- an optional `USING BASELINE` clause
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

This becomes a sequence of historical SPARQL executions over overlapping or stepped windows.

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

## What Janus Generates Internally

The parser splits the query into:

- one live RSP-QL query built from live windows
- one SPARQL query per historical window

Important detail:

- non-window patterns in the `WHERE` clause are preserved in the live query
- this is what makes baseline joins like `?sensor baseline:mean ?mean` work during live execution

## Baseline Predicates

Baseline values are exposed to the live side as static triples under:

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

## Practical Guidance

- Use fixed historical windows when you want one clean baseline snapshot.
- Use historical sliding windows only when you really need a baseline derived from multiple historical subwindows.
- Keep historical baseline queries compact. Prefer one row per anchor such as one row per sensor.
