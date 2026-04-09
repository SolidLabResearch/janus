# Query Execution

This document describes how Janus executes a registered query.

## Main Components

- `JanusQLParser`: parses Janus-QL and derives live and historical query fragments
- `QueryRegistry`: stores registered query metadata
- `JanusApi`: coordinates startup, workers, status, and result delivery
- `HistoricalExecutor`: executes SPARQL over stored historical data
- `LiveStreamProcessing`: executes the live RSP-QL query and holds live/static data
- HTTP server: exposes registration, control, inspection, and WebSocket result streaming

## Registration

Registration stores:

- the raw Janus-QL query text
- the parsed query representation
- the default `baseline_mode`
- timestamps and execution counters

Registration does not start execution.

## Startup Flow

`JanusApi::start_query()` does the following:

1. Loads query metadata from the registry.
2. Checks whether the query is already running.
3. Creates one result channel for both historical and live results.
4. Spawns one historical worker per historical window.
5. Starts the live processor if the query has live windows.
6. Starts async baseline warm-up if the query has both historical and live windows.
7. Stores runtime handles and returns a `QueryHandle`.

## Historical Execution

Historical execution is per historical window.

### Fixed Historical Window

- one SPARQL execution
- one batch of historical bindings
- one `QueryResult` sent with source `Historical`

### Sliding Historical Window

- multiple SPARQL executions, one per computed window
- one result batch per window
- each batch sent with source `Historical`

Historical execution is not merged into live state automatically unless baseline bootstrap is used.

## Live Execution

Live execution uses `LiveStreamProcessing`.

Startup does the following:

- creates the live processor from generated RSP-QL
- registers all live streams referenced by live windows
- starts the live processor
- spawns MQTT subscribers for each live stream
- spawns a worker that forwards emitted live bindings as `QueryResult { source: Live }`

## Result Delivery

Janus multiplexes historical and live results onto a single channel.

Each `QueryResult` contains:

- `query_id`
- `timestamp`
- `source`: `Historical` or `Live`
- `bindings`

The HTTP server forwards these results into a broadcast channel and exposes them over WebSocket.

## Runtime Status

Current execution states are:

- `Registered`
- `WarmingBaseline`
- `Running`
- `Stopped`
- `Failed(String)`
- `Completed`

Important behavior:

- hybrid live + historical baseline queries begin in `WarmingBaseline`
- live execution starts immediately
- status flips to `Running` when baseline warm-up finishes successfully

## What State Janus Keeps

Janus keeps runtime state for:

- worker thread handles
- shutdown channels
- live processors
- MQTT subscribers
- query execution status

Janus does not keep a fully merged historical + live relation as one continuously maintained execution state.

If you use baselines, Janus keeps only the compact materialized baseline triples inside live static data, not all historical result rows.
