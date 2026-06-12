# Baselines

Baseline support in Janus is meant for hybrid anomaly-style queries where historical data initializes context for live scoring.

It is not a full hybrid-state engine.

Janus currently supports two baseline paths:

- legacy `USING BASELINE <historical-window> LAST|AGGREGATE`
- query-defined `DEFINE BASELINE :name ON WINDOW :history AS SELECT ...` together with `USING BASELINE :name`

## What Baseline Bootstrap Does

When a query has:

- at least one historical window
- at least one live window
- a baseline-aware query shape

Janus can evaluate the historical side, collapse the result into compact baseline statements, and insert those statements into the live processor as static data.

The live query then joins against those static triples.

## How It Is Enabled

Preferred query-level form:

```sparql
USING BASELINE ex:hist LAST
```

or:

```sparql
USING BASELINE ex:hist AGGREGATE
```

If the clause is missing, registration can still provide:

- `baseline_mode = aggregate`
- `baseline_mode = last`

The query-level clause takes precedence when present.

For query-defined baselines, `USING BASELINE :name` means:

1. evaluate the generated historical baseline query over its source `LOG` window
2. materialize the resulting bindings through the matching `GRAPH :name { ... }` template
3. insert the resulting RDF quads into the live RSP engine static store before live startup
4. allow the live query to bind those values during live execution

## Canonical Query-Defined Example

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

This compares the 1-minute live average against the 1-day historical average for each sensor. The historical `AVG` result is turned into static RDF before the live query starts, so `?dayAvgValue` is an ordinary live-side binding once the query runs.

## Query-Defined Materialization Rule

Current rule in v0.1:

- the `GRAPH :baselineName { ... }` block defines the RDF shape that is inserted
- the baseline query must project every variable used by that template
- template predicates must be concrete IRIs
- template subjects must resolve to an IRI or blank node

Example:

- `?sensor = :s1`
- `?dayAvgValue = 42.0`

becomes:

```trig
GRAPH :dayBaseline {
  :s1 :dayAvgValue 42.0 .
}
```

This is why Janus uses the `GRAPH` template rather than trying to infer quads from `SELECT` projection aliases: the template states the exact RDF layout that should be injected, while `SELECT` only describes the available bindings.

## LAST vs AGGREGATE

### LAST

For a historical sliding window:

- only the final sliding-window result snapshot is retained
- earlier window outputs are discarded for baseline collapse

This is useful when you want:

- the most recent historical regime
- a low-ambiguity startup baseline

### AGGREGATE

For a historical sliding window:

- all historical sliding-window outputs are folded into one compact baseline
- numeric values are averaged per `(anchor, variable)`
- non-numeric values fall back to the latest seen value

This is useful when you want:

- a broader recent historical summary
- less sensitivity to the last historical subwindow

## Fixed Historical Windows

For a fixed historical window, the distinction between `LAST` and `AGGREGATE` is much smaller because there is only one historical result set.

In practice:

- fixed historical baseline is usually the simplest and clearest baseline path
- historical sliding baseline is more advanced and can cost more at startup

## Startup Behavior

Legacy window baselines warm asynchronously.

Behavior for `USING BASELINE <window> LAST|AGGREGATE`:

1. live execution starts immediately
2. query status becomes `WarmingBaseline`
3. baseline bootstrap runs in a background thread
4. baseline triples are inserted into live static data
5. query status moves to `Running`

Effect on query results:

- a live query that depends on baseline joins typically produces no matches until the baseline is ready
- once baseline static data exists, future live evaluations can match those joins

Behavior for query-defined baselines:

1. the generated historical baseline query is evaluated before live startup
2. the result bindings are materialized into named-graph static quads
3. those quads are injected into the live RSP engine before `start_processing()`
4. the live query starts only after that injection succeeds

## What Janus Stores

Janus does not retain all historical events or all historical sliding-window outputs as permanent runtime state.

For baseline bootstrap it retains:

- a compact accumulator keyed by `(anchor, variable)` during bootstrap
- then final static baseline triples inside live processing

It does not retain:

- all raw historical events in memory
- all sliding-window result batches after bootstrap
- a continuously merged historical/live relation

For query-defined baselines it retains:

- the evaluated historical bindings per baseline name
- the final materialized baseline quads inside the live static store

## Limitations

- query-defined baselines are startup snapshots, not continuously updated historical/live state
- the `GRAPH` template predicates must be concrete IRIs
- template variables must be projected by the baseline query
- template subjects must resolve to an IRI or blank node
- large baseline result sets can make static-store injection expensive at startup

## Recommended Usage

- Prefer fixed historical windows first.
- Use historical sliding windows only when you need a baseline derived from multiple historical subwindows.
- Keep baseline queries compact, ideally one row per anchor.
- Start with compact baseline values such as `mean`, `sigma`, or `dayAvgValue`; add more only when needed.
