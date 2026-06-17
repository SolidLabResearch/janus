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
2. store the `SELECT` result as a named baseline snapshot keyed by evaluation time
3. resolve the correct snapshot for each live evaluation timestamp
4. expose that snapshot to the live query through `USING BASELINE :name`

For compatibility with the current live query shape, Janus still materializes evaluation-local
quads from the snapshot's binding rows when the live query contains a matching
`GRAPH :name { ... }` block. The snapshot remains the authoritative stored form.

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

This compares the 1-minute live average against the 1-day historical average for each sensor.

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

This is why Janus uses the `GRAPH` template rather than trying to infer quads from `SELECT`
projection aliases: the template states the exact RDF layout for the evaluation-local join view,
while `SELECT` describes the stored binding rows.

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

For a fixed historical window, Janus resolves the same absolute interval for every live
evaluation and computes one reusable baseline snapshot.

For a sliding historical window:

- at live evaluation time `T`, Janus resolves `[T - OFFSET - RANGE, T - OFFSET]`
- the baseline snapshot is recomputed or refreshed for that `T`
- snapshots are versioned by baseline id and evaluation timestamp

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

1. fixed historical baselines are computed once and stored as snapshots
2. sliding historical baselines are resolved per live evaluation timestamp
3. Janus keeps the stored form as binding-row snapshots, not live stream events
4. the live query joins against the snapshot selected for that evaluation

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

- the evaluated historical binding rows per baseline name and evaluation timestamp
- source-window metadata for each snapshot
- an evaluation-local materialized join view only when live execution needs it

## Limitations

- query-defined baselines support `SELECT` snapshots only
- `CONSTRUCT` baselines are not supported
- sliding historical baseline `STEP` must match the live `STEP`
- the `GRAPH` template predicates must be concrete IRIs
- template variables must be projected by the baseline query
- template subjects must resolve to an IRI or blank node
- the current implementation is tested for projected variables, `AVG`, `GROUP BY`, and arithmetic
  joins in the live query

## Recommended Usage

- Prefer fixed historical windows first.
- Use historical sliding windows only when you need a baseline derived from multiple historical subwindows.
- Keep baseline queries compact, ideally one row per anchor.
- Start with compact baseline values such as `mean`, `sigma`, or `dayAvgValue`; add more only when needed.
