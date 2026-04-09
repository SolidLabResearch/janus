# Baselines

Baseline support in Janus is meant for hybrid anomaly-style queries where historical data initializes context for live scoring.

It is not a full hybrid-state engine.

## What Baseline Bootstrap Does

When a query has:

- at least one historical window
- at least one live window
- a baseline-aware query shape, typically with `baseline:*` joins

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

## Async Warm-Up

Janus now warms baseline state asynchronously.

Behavior:

1. live execution starts immediately
2. query status becomes `WarmingBaseline`
3. baseline bootstrap runs in a background thread
4. baseline triples are inserted into live static data
5. query status moves to `Running`

Effect on query results:

- a live query that depends on baseline joins typically produces no matches until the baseline is ready
- once baseline static data exists, future live evaluations can match those joins

## What Janus Stores

Janus does not retain all historical events or all historical sliding-window outputs as permanent runtime state.

For baseline bootstrap it retains:

- a compact accumulator keyed by `(anchor, variable)` during bootstrap
- then final static baseline triples inside live processing

It does not retain:

- all raw historical events in memory
- all sliding-window result batches after bootstrap
- a continuously merged historical/live relation

## Anchor Selection

Baseline values are materialized per anchor subject.

The current implementation prefers binding variables named:

- `sensor`
- `subject`
- `entity`
- `s`

If none of those exist, Janus falls back to the first IRI-like binding it can find.

This means historical baseline queries work best when they explicitly return a stable anchor variable such as `?sensor`.

## Recommended Usage

- Prefer fixed historical windows first.
- Use historical sliding windows only when you need a baseline derived from multiple historical subwindows.
- Keep baseline queries compact, ideally one row per anchor.
- Start with baseline values such as `mean` and `sigma`; add `slope` or quantiles later if needed.
