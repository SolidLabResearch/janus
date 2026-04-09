# Anomaly Detection

Janus already supports anomaly-oriented extension functions, but they are stateless functions evaluated within one query execution context.

That distinction matters.

## What Extension Functions Are Good At

Current extension functions are sufficient for:

- fixed thresholds
- relative change checks
- z-score style checks when mean and sigma are already present
- simple outlier or divergence predicates over current bindings

This works well when the query already has everything it needs in one evaluation context.

## Where Baselines Help

Baselines help when live anomaly scoring depends on historical context such as:

- deviation from normal behavior
- per-sensor baselines
- volatility comparison
- recent historical trend

In those cases, Janus can bootstrap compact historical values into live static data and let the live query compare current readings against them.

## What Janus Does Not Do

Janus does not currently maintain a full continuously updated hybrid historical/live relation.

So if you need:

- long-running stateful models
- full seasonal context
- large retained historical buffers inside the engine

you will need either:

- external model state
- future dedicated baseline refresh logic
- more specialized stateful operators

## Recommended Pattern

For a first anomaly-detection pipeline in Janus:

1. Use a historical query that emits one compact row per anchor.
2. Materialize baseline values such as `mean` and `sigma`.
3. Join those values in the live query using `baseline:*` predicates.
4. Apply extension functions on the live side.

Example:

```sparql
PREFIX ex: <http://example.org/>
PREFIX janus: <https://janus.rs/fn#>
PREFIX baseline: <https://janus.rs/baseline#>

REGISTER RStream ex:out AS
SELECT ?sensor ?reading
FROM NAMED WINDOW ex:hist ON LOG ex:store [START 1700000000000 END 1700003600000]
FROM NAMED WINDOW ex:live ON STREAM ex:stream1 [RANGE 5000 STEP 1000]
USING BASELINE ex:hist LAST
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

## Choosing LAST vs AGGREGATE

- Use `LAST` when you care about the most recent historical regime before live execution.
- Use `AGGREGATE` when you want a more stable summary across multiple historical sliding windows.
- Prefer fixed historical windows unless you have a clear reason to derive a baseline from many historical subwindows.
