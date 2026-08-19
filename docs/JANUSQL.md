# Janus-QL

Janus-QL is Janus's extension for evaluating RDF queries over historical event
logs, live streams, or both. It keeps the SPARQL `SELECT`/`WHERE` shape and
adds named-window declarations plus an optional `REGISTER RStream` wrapper for
live result streams.

This page describes the public, tested query surface. It deliberately does not
attempt to reproduce all of SPARQL or RSP-QL.

## Query shape

```sparql
PREFIX ex: <http://example.org/>

REGISTER RStream ex:output AS
SELECT ?sensor ?value
FROM NAMED WINDOW ex:live ON STREAM ex:stream [RANGE 60000 STEP 30000]
FROM NAMED WINDOW ex:history ON LOG ex:log [START 0 END 86400000]
WHERE {
  WINDOW ex:live {
    ?sensor ex:hasValue ?value .
  }
}
```

- `PREFIX` declarations are optional, but usually make queries readable.
- `REGISTER RStream <output> AS` is used for a query that emits live results.
  `IStream` and `DStream` are rejected.
- A historical-only `SELECT` query may omit `REGISTER`.
- Each `WINDOW <name> { … }` in the `WHERE` clause must refer to a declared
  named window.

## Window declarations

### Fixed historical window

Use `ON LOG` with an explicit, non-empty `[START … END …]` interval:

```sparql
FROM NAMED WINDOW ex:history ON LOG ex:log [START 1700000000000 END 1700086400000]
```

Janus evaluates this window against the segmented historical event log.

### Sliding historical window

Use `ON LOG` with `OFFSET`, `RANGE`, and `STEP`:

```sparql
FROM NAMED WINDOW ex:previousHour ON LOG ex:log [OFFSET 86400000 RANGE 3600000 STEP 30000]
```

At evaluation time `T`, the window is:

```text
[T - OFFSET - RANGE, T - OFFSET]
```

The range must not exceed the offset; Janus rejects a sliding historical
window that would extend beyond its evaluation time.

### Sliding live window

Use `ON STREAM` with positive `RANGE` and `STEP` values:

```sparql
FROM NAMED WINDOW ex:live ON STREAM ex:stream [RANGE 60000 STEP 30000]
```

The stream URI identifies the live source; at the HTTP API layer, live
execution is MQTT-backed. See [the live streaming guide](./LIVE_STREAMING_GUIDE.md)
for broker and replay setup.

## Hybrid and nested-historical queries

A query may declare both log and stream windows. The parser lowers historical
work to one or more SPARQL executions and keeps live windows in the RSP-QL
execution path.

Historical work can also be expressed as a nested `SELECT` inside the outer
`WHERE` clause:

```sparql
PREFIX ex: <http://example.org/>

REGISTER RStream ex:output AS
SELECT ?sensor ?value ?historicalAverage
FROM NAMED WINDOW ex:live ON STREAM ex:stream [RANGE 60000 STEP 30000]
FROM NAMED WINDOW ex:history ON LOG ex:log [START 0 END 86400000]
WHERE {
  WINDOW ex:live { ?sensor ex:hasValue ?value . }
  {
    SELECT ?sensor (AVG(?oldValue) AS ?historicalAverage)
    WHERE {
      WINDOW ex:history { ?sensor ex:hasValue ?oldValue . }
    }
    GROUP BY ?sensor
  }
  FILTER(?value > ?historicalAverage)
}
```

Nested historical materialization is restricted to supported historical
shapes. A nested query that is live-only, or mixes live and historical windows
inside the same nested subquery, is rejected. Details and limitations are in
[Nested Historical Subqueries](./NESTED_HISTORICAL_SUBQUERIES.md).

## Validation boundaries

Janus validates the Janus-specific surface before execution. In particular,
it rejects:

- undeclared `WINDOW` names;
- `START`/`END` bounds on a stream window;
- `RANGE`/`STEP` live bounds on a log window;
- zero or invalid window sizes and steps;
- `IStream` or `DStream` registration; and
- unsupported property paths and `SERVICE` patterns.

The parser is not a claim of general SPARQL or RSP-QL equivalence. Use only
the syntax exercised by the repository's parser and integration tests.

## Implementation compatibility features

The implementation retains baseline-oriented compatibility paths used by
some benchmark and API code. They are not part of the public Janus-QL surface
described above. Their operational constraints are documented separately in
[Baselines](./BASELINES.md); do not use them as a portability guarantee.

## Related documentation

- [Query Execution](./QUERY_EXECUTION.md)
- [HTTP API](./HTTP_API_CURRENT.md)
- [Window Types Explained](./WINDOW_TYPES_EXPLAINED.md)
- [Nested Historical Subqueries](./NESTED_HISTORICAL_SUBQUERIES.md)
