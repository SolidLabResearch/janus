# CityBench-Inspired Congestion Hybrid Scaling Benchmark

## 1. Benchmark purpose

`hybrid_scaling_combined` evaluates historical-live RDF stream processing on a deterministic CityBench-inspired congestion workload. The benchmark compares unified Janus execution against a decomposed Oxigraph baseline while scaling the size of the historical RDF event log.

The benchmark reuses one deterministic historical congestion model and one deterministic live congestion model, then runs both systems over the same generated data for each configuration.

## 2. Systems compared

### Janus unified execution

Janus unified execution uses `StreamingSegmentedStorage` as the historical RDF event log and evaluates the historical congestion aggregate through Janus's lowered historical subquery path. Janus materializes the resulting per-sensor `historicalAvgCongestion` bindings into static named-graph context for live execution, then evaluates the live Janus query over the current live window together with that internal materialized historical context.

This path does use Janus segmented RDF event-log access for historical retrieval.

### Decomposed Oxigraph baseline

The decomposed baseline loads the historical RDF data into an Oxigraph store, evaluates the historical component separately with SPARQL using explicit timestamp filtering, evaluates the live component separately with a live-only Janus window query, then joins the historical and live results externally in Rust and applies the comparison predicate externally.

This path does not use Janus segmented historical lookup for historical filtering.

## 3. Dataset and event model

### RDF identifiers and literals

- Historical graph IRI: `http://example.org/citybench`
- Live stream IRI registered in Janus: `http://example.org/live`
- Sensor IRIs: `http://example.org/junction/{i % 8}`
- Congestion predicate: `http://example.org/congestionLevel`
- Historical and live congestion values are emitted as typed literals with datatype `http://www.w3.org/2001/XMLSchema#decimal`

### Timestamp model

The benchmark uses event timestamps embedded in `RDFEvent`.

- Historical benchmark data starts at `1_800_500_000_000` ms in `hybrid_scaling_combined`
- Historical events are spaced every `60` ms
- Live events start at `1_900_000_000_000` ms
- Live events are spaced every `event_interval_ms`, currently `250` ms

Historical timestamp for event index `i`:

```text
ts_h(i) = historical_base_ts + i * 60
```

Live timestamp for event index `i`:

```text
ts_l(i) = live_start_ts + i * event_interval_ms
```

### Number of sensors

The generated workload uses `8` sensors.

### Historical event generation

For historical event index `index`:

```text
sensor_idx = index % 8
sample_idx = index / 8
historical_sensor_base(sensor_idx) = 30.0 + sensor_idx * 5.0
seasonal = ((sample_idx * 7 + sensor_idx * 3) % 9) - 4.0
historical_congestion(index) = historical_sensor_base(sensor_idx) + seasonal
```

Each historical RDF event is:

```text
timestamp = historical_base_ts + index * 60
subject   = http://example.org/junction/{index % 8}
predicate = http://example.org/congestionLevel
object    = decimal literal formatted to 3 fractional digits
graph     = http://example.org/citybench
```

### Live event generation

For live event index `index`:

```text
sensor_idx = index % 8
sample_idx = index / 8
historical_sensor_base(sensor_idx) = 30.0 + sensor_idx * 5.0
bias = 8.0 if sensor_idx is even, else -8.0
oscillation = ((sample_idx * 5 + sensor_idx) % 5) - 2.0
live_congestion(index) = historical_sensor_base(sensor_idx) + bias + oscillation
```

Each live RDF event is:

```text
timestamp = live_start_ts + index * event_interval_ms
subject   = http://example.org/junction/{index % 8}
predicate = http://example.org/congestionLevel
object    = decimal literal formatted to 3 fractional digits
graph     = http://example.org/citybench
```

Although the live query registers the stream as `http://example.org/live`, the event objects themselves are generated with the same RDF subject/predicate/object/graph pattern as the historical events, and the events are fed to that registered live stream at replay time.

### Determinism and cross-system data reuse

The workload is deterministic because both the historical and live values are pure functions of the event index, the timestamp bases are fixed constants, and the sensor mapping is fixed modulo `8`.

For each configuration:

- historical storage is generated once;
- the same historical event sequence is reused for both systems;
- the same live event vector is reused for both systems.

Janus reads historical events from segmented storage. The decomposed baseline obtains the same historical events through `historical_storage.query_rdf(0, u64::MAX)` and loads them into Oxigraph.

### Why this is CityBench-inspired

The workload keeps the traffic-monitoring pattern of comparing live congestion against historical congestion per sensor, but it is not the original CityBench dataset. It is a synthetic, deterministic CityBench-inspired workload that preserves the aggregate congestion-monitoring shape while allowing controlled scaling of the historical RDF event log.

## 4. Query workload

### Janus historical context computation

The Janus path does not evaluate the historical average as a generic SPARQL aggregate over returned quads. After the historical range lookup returns RDF events, the benchmark computes the per-sensor historical average directly over the retrieved event sequence:

```text
for each event:
  parse event.object as f64
  accumulate sum and count by event.subject

for each sensor:
  historicalAvgCongestion = sum / count
```

This produces rows of the form:

```text
{ sensor, historicalAvgCongestion }
```

Those rows are then materialized into Janus's internal lowered named-graph context for live execution:

```text
GRAPH <lowered-materialized-history-graph> {
  <sensor> <lowered-materialized-history-predicate> "value"
}
```

### Janus live hybrid query

The benchmark builds this Janus query template:

```sparql
PREFIX ex: <http://example.org/>

REGISTER RStream <output> AS
SELECT ?sensor
       ?liveAvgCongestion
       ?historicalAvgCongestion
       ((?liveAvgCongestion - ?historicalAvgCongestion) AS ?congestionDelta)
FROM NAMED WINDOW ex:hist ON LOG <http://example.org/citybench> [START T_START END T_END]
FROM NAMED WINDOW ex:live ON STREAM <http://example.org/live> [RANGE WINDOW_SIZE_MS STEP WINDOW_SLIDE_MS]
WHERE {
  {
    SELECT ?sensor
           (AVG(?historicalCongestion) AS ?historicalAvgCongestion)
    WHERE {
      WINDOW ex:hist {
        ?sensor ex:congestionLevel ?historicalCongestion .
      }
    }
    GROUP BY ?sensor
  }
  {
    SELECT ?sensor
           (AVG(?liveCongestion) AS ?liveAvgCongestion)
    WHERE {
      WINDOW ex:live {
        ?sensor ex:congestionLevel ?liveCongestion .
      }
    }
    GROUP BY ?sensor
  }
  FILTER(?liveAvgCongestion > ?historicalAvgCongestion)
}
```

In the actual execution path, the query string uses `END = end_ts - 1` because the internal historical lookup is treated as half-open `[start_ts, end_ts)`. Janus lowers the historical subquery internally into materialized named-graph context for live execution; that lowering is an execution strategy, not user-facing Janus-QL syntax.

### Decomposed historical SPARQL query

The decomposed baseline builds this SPARQL template:

```sparql
PREFIX ex: <http://example.org/>
PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>
SELECT ?sensor (AVG(?historicalCongestion) AS ?historicalAvgCongestion) WHERE {
    GRAPH ?eventGraph {
        ?sensor ex:congestionLevel ?historicalCongestion .
    }
    ?eventGraph ex:timestamp ?t .
    ?eventGraph ex:graph <http://example.org/citybench> .
    FILTER(?t >= T_START && ?t <= T_END)
}
GROUP BY ?sensor
```

Oxigraph stores each historical observation in an event-specific named graph `http://example.org/event/{index}` and stores the timestamp and original graph metadata in the default graph.

### Decomposed live query

The decomposed live side uses this live-only Janus query template:

```sparql
PREFIX ex: <http://example.org/>

REGISTER RStream <output> AS
SELECT ?sensor
       (AVG(?liveCongestion) AS ?liveAvgCongestion)
FROM NAMED WINDOW ex:live ON STREAM <http://example.org/live> [RANGE WINDOW_SIZE_MS STEP WINDOW_SLIDE_MS]
WHERE {
    WINDOW ex:live {
        ?sensor ex:congestionLevel ?liveCongestion .
    }
}
GROUP BY ?sensor
```

### External join and filter logic

The decomposed baseline performs the final hybrid step outside Oxigraph and outside the live query engine:

```text
baseline_by_sensor = map historical row by normalized sensor IRI

for each live row:
  lookup matching historical row by sensor
  live_val = liveAvgCongestion
  base_val = historicalAvgCongestion
  if live_val > base_val:
    emit merged row
    emit congestionDelta = live_val - base_val
```

The implemented comparison predicate is therefore:

```text
AVG(?liveCongestion) > ?historicalAvgCongestion
```

The Janus query uses that predicate in `HAVING(...)`, and the decomposed baseline applies the same predicate after the external join.

## 5. Historical access patterns

For a historical size `h`, the benchmark defines:

```text
ts(i) = base_ts + i * 60
```

and then chooses `(start_ts, end_ts)` as follows:

- `point_lookup`: `(ts(h - 1), ts(h))`
- `fixed_60s`: `(ts(h - 1000), ts(h))`
- `range_10_percent`: `(ts(h - h / 10), ts(h))`
- `range_50_percent`: `(ts(h - h / 2), ts(h))`
- `range_100_percent`: `(ts(0), ts(h))`

### Internal interval semantics

Internally the benchmark treats the historical range as half-open:

```text
[start_ts, end_ts)
```

That is documented directly in the implementation comments, and both execution paths translate it by querying with `end_ts - 1`:

- Janus historical lookup: `historical_storage.query_rdf(start_ts, end_ts - 1)`
- Decomposed SPARQL filter: `FILTER(?t >= start_ts && ?t <= end_ts - 1)`
- Janus query string: historical `END` placeholder is filled with `end_ts - 1`

So the user-facing query text looks closed on the upper bound, but it is implementing the half-open interval `[start_ts, end_ts)`.

### Pattern-by-pattern behavior

#### `point_lookup`

- Start timestamp: last event timestamp `ts(h - 1)`
- End timestamp: one event step later `ts(h)`
- Effective selected events: `1`
- Translation: `query_rdf(start, end - 1)` and `FILTER(?t >= start && ?t <= end - 1)`
- Expected selectivity: constant, independent of historical log size

#### `fixed_60s`

- Start timestamp: `ts(h - 1000)`
- End timestamp: `ts(h)`
- Effective selected events: `1000.min(h)`
- Because each historical event is spaced every `60` ms, this corresponds to `1000 * 60 ms = 60000 ms`
- Translation: same half-open range lowered as `end - 1`
- Expected selectivity: bounded constant-size slice once `h >= 1000`

#### `range_10_percent`

- Start timestamp: `ts(h - h / 10)`
- End timestamp: `ts(h)`
- Effective selected events: `(h / 10).max(1)`
- Translation: same half-open range lowered as `end - 1`
- Expected selectivity: 10% of the historical log

#### `range_50_percent`

- Start timestamp: `ts(h - h / 2)`
- End timestamp: `ts(h)`
- Effective selected events: `(h / 2).max(1)`
- Translation: same half-open range lowered as `end - 1`
- Expected selectivity: 50% of the historical log

#### `range_100_percent`

- Start timestamp: `ts(0)`
- End timestamp: `ts(h)`
- Effective selected events: `h.max(1)`
- Translation: same half-open range lowered as `end - 1`
- Expected selectivity: full historical log

### Result-count expectation used by the benchmark

The benchmark records:

```text
expected_historical_result_count = min(selected_historical_events, 8)
```

because both systems aggregate per sensor, and there are `8` sensor IRIs.

## 6. Experimental parameters

### Current paper experiment parameters in code/local run

- historical sizes: `10000,50000,100000,500000`
- query types: `point_lookup,fixed_60s,range_10_percent,range_50_percent,range_100_percent`
- systems: `janus,decomposed_oxigraph`
- event rate: `4`
- event interval: `250 ms`
- live window size: currently tested `10000 ms`
- live window slide: currently tested `5000 ms`
- measured iterations in the provided local command: `5`
- release mode: yes

The binary defaults are not identical to the provided local command:

- CLI default `iterations`: `30`
- CLI default `live_duration_ms`: `20000`
- CLI default `window_size_ms`: `10000`
- CLI default `window_slide_ms`: `5000`

### Proposed final server run parameters

- iterations: `35`
- live window size: `60000 ms`
- live window slide: `30000 ms`
- recommended live duration: `180000 ms`

`180000 ms` is appropriate because with a `60000 ms` window and `30000 ms` slide it permits multiple completed windows after the first full window closes. In particular, it is comfortably above the minimum needed for at least two completed windows and should produce at least the windows ending around `60000 ms`, `90000 ms`, `120000 ms`, `150000 ms`, and `180000 ms`, subject to the engine’s normal window-closing behavior.

## 7. Metrics

### `summary.csv`

The benchmark writes `hybrid_scaling_combined.summary.csv` with these columns:

- `historical_query_type`
- `historical_size_quads`
- `system`
- `first_hybrid_result_mean_ms`
- `first_hybrid_result_std_ms`
- `main_window_result_mean_ms`
- `main_window_result_std_ms`
- `historical_lookup_mean_ms`
- `historical_lookup_std_ms`
- `window_overhead_mean_ms`
- `window_overhead_std_ms`
- `external_merge_mean_ms`
- `external_merge_std_ms`
- `equivalence_rate`
- `peak_rss_mean`
- `peak_rss_std`
- `rss_delta_mean`
- `rss_delta_std`
- `mean_cpu_mean`
- `mean_cpu_std`
- `peak_cpu_mean`
- `peak_cpu_std`

Field meanings:

- `historical_lookup_mean_ms`: mean time spent evaluating the historical side for that system. For Janus this is the range lookup plus in-process baseline computation over retrieved events. For the decomposed baseline this is the Oxigraph historical SPARQL evaluation time. It does not include Oxigraph load time in this summary column.
- `window_overhead_mean_ms`: mean of `post_trigger_result_observation_delay_ms`, which is the measured delay around result observation after `add_event` and short polling. It is not a full end-to-end live-window schedule metric.
- `external_merge_mean_ms`: mean external join/filter time. Janus writes `0.0`; the decomposed baseline reports the measured Rust-side merge time for the first recorded hybrid window result.
- `first_hybrid_result_mean_ms`: mean elapsed replay time to the first completed hybrid result observed for the first unique slide timestamp.
- `main_window_result_mean_ms`: mean elapsed replay time to the first result observed for the second unique slide timestamp.
- `equivalence_rate`: fraction of measured iterations for that `(query_type, size, system)` whose `result_equivalence` flag is true.
- `peak_rss_mean`: mean peak process RSS in MB over the sampled run.
- `rss_delta_mean`: mean difference `rss_end_mb - rss_start_mb`.
- `mean_cpu_mean`: mean sampled process CPU percent over the run.
- `peak_cpu_mean`: mean of each run’s peak sampled process CPU percent.

### `raw.jsonl`

The benchmark writes one JSON object per run to `hybrid_scaling_combined.raw.jsonl`. The row schema is `HybridScalingRow` and includes:

- benchmark identity: `benchmark_name`, `system`, `historical_query_type`, `query_name`, `iteration`, `timestamp`
- historical dataset parameters: `historical_size_quads`, `target_historical_quads`, `actual_historical_quads`
- historical query shape metadata: `expected_historical_result_count`, `historical_result_count`, `historical_result_count_matches_expected`, `historical_query_start_ms`, `historical_query_end_ms`, `historical_query_span_ms`
- live workload parameters: `live_duration_ms`, `event_rate_per_second`, `event_interval_ms`, `total_live_events`, `window_size_ms`, `window_slide_ms`
- timing metrics: `registration_ms`, `historical_lookup_ms`, `historical_query_ms`, `first_live_window_result_ms`, `first_hybrid_result_ms`, `main_window_result_ms`, `first_hybrid_window_adjusted_overhead_ms`, `main_window_adjusted_overhead_ms`, `window_processing_overhead_ms`, `post_trigger_result_observation_delay_ms`, `external_merge_ms`, `total_run_ms`
- correctness fields: `result_count`, `result_hash`, `matching_reference_hash`, `result_equivalence`, `mismatch_reason`
- resource metrics: `rss_start_mb`, `rss_end_mb`, `peak_rss_mb`, `rss_delta_mb`, `mean_cpu_percent`, `peak_cpu_percent`, `resource_sample_count`, `resource_sample_interval_ms`
- historical-side implementation metadata: `historical_backend`, `historical_query_language`, `historical_load_ms`

Important details:

- `historical_lookup_ms` and `historical_query_ms` currently carry the same value in both execution paths.
- `historical_load_ms` is only non-zero for the decomposed Oxigraph path because it builds an in-memory Oxigraph store from the historical event sequence on each run.
- `first_live_window_result_ms` is only populated for the decomposed path.
- `matching_reference_hash` is currently left empty by this benchmark.

### `results.md`

The benchmark writes `hybrid_scaling_combined_results.md`, which contains grouped Markdown tables derived from the raw rows:

- end-to-end hybrid latency table
- historical access scaling table
- historical result counts table
- result equivalence table
- process-level resource table

### Resource-measurement limitation

Resource measurements are process-level samples collected by `sysinfo` inside one long-running process. RSS can therefore be affected by allocator retention and prior configurations. The benchmark’s own generated results Markdown explicitly notes this limitation.

## 8. Correctness/equivalence check

### Canonicalization and hashing

Result equivalence is computed by canonicalizing result rows and hashing them with SHA-256.

For each result row:

- normalize RDF term renderings by stripping surrounding `<...>` or typed-literal wrappers;
- if a normalized value parses as `f64`, format it to six fractional digits;
- sort key-value pairs within the row;
- sort rows globally;
- serialize the canonical row list as JSON;
- hash the JSON payload with SHA-256.

The benchmark then marks the Janus and decomposed rows equivalent only if:

```text
janus.result_hash == decomposed.result_hash
and
janus.result_count == decomposed.result_count
```

### Meaning of `equivalence_rate = 1.000`

`equivalence_rate = 1.000` in `summary.csv` means every measured run for that grouped configuration had `result_equivalence = true`.

### Failure checks

Use:

```bash
grep -R '"result_equivalence":false' <output-dir>
grep -R "correctness_passed=false" <output-dir>
```

If equivalence fails, inspect:

- `<output-dir>/hybrid_scaling_combined.raw.jsonl`
- `<output-dir>/hybrid_scaling_combined.raw.json`
- `<output-dir>/hybrid_scaling_combined.summary.csv`
- `<output-dir>/hybrid_scaling_combined_results.md`

The raw rows contain `mismatch_reason`, `result_count`, and `result_hash` for each run.

## 9. Commands

### A. Local smoke command

```bash
cargo run --release --bin hybrid_scaling_combined -- \
  --historical-sizes 10000 \
  --historical-query-types point_lookup,fixed_60s \
  --iterations 1 \
  --live-duration-ms 20000 \
  --event-rate 4 \
  --event-interval-ms 250 \
  --window-size-ms 10000 \
  --window-slide-ms 5000 \
  --systems janus,decomposed_oxigraph \
  --output logs/benchmark/hybrid_scaling_combined/citybench_congestion_smoke
```

### B. Existing local 5-iteration command

```bash
cargo run --release --bin hybrid_scaling_combined -- \
  --historical-sizes 10000,50000,100000,500000 \
  --historical-query-types point_lookup,fixed_60s,range_10_percent,range_50_percent,range_100_percent \
  --iterations 5 \
  --live-duration-ms 20000 \
  --event-rate 4 \
  --event-interval-ms 250 \
  --window-size-ms 10000 \
  --window-slide-ms 5000 \
  --systems janus,decomposed_oxigraph \
  --output logs/benchmark/hybrid_scaling_combined/citybench_congestion_all_queries_5iter
```

### C. Proposed final server 35-iteration command

```bash
cargo run --release --bin hybrid_scaling_combined -- \
  --historical-sizes 10000,50000,100000,500000 \
  --historical-query-types point_lookup,fixed_60s,range_10_percent,range_50_percent,range_100_percent \
  --iterations 35 \
  --live-duration-ms 180000 \
  --event-rate 4 \
  --event-interval-ms 250 \
  --window-size-ms 60000 \
  --window-slide-ms 30000 \
  --systems janus,decomposed_oxigraph \
  --output logs/benchmark/hybrid_scaling_combined/citybench_congestion_all_queries_35iter_60s_30s
```

## 10. Paper reporting guidance

- Use an equivalence table covering all query types and historical sizes.
- Use `historical_lookup_mean_ms` as the main scaling result table because it isolates the historical side more cleanly than the first-result and second-window timings.
- Do not overemphasize `first_hybrid_result_mean_ms` or `main_window_result_mean_ms`; in this benchmark family they are strongly influenced by the live window schedule.
- Report `external_merge_mean_ms` for the decomposed execution because that cost is part of the decomposed architecture and is absent in unified Janus execution.
- Mention the process-level memory limitation carefully when discussing RSS numbers.

## 11. Known implementation note

The repository already records a known issue around duplicate RDF observations in generic historical SPARQL-style execution paths: if repeated observations have identical subject, predicate, object, and graph, a generic historical SPARQL executor can collapse duplicate RDF quads and therefore lose event multiplicity for aggregates.

This combined benchmark avoids that issue on the Janus unified side by:

- retrieving historical RDF events from the segmented event log;
- computing the historical congestion context directly over the retrieved event sequence before materialization;
- materializing only the aggregated `historicalAvgCongestion` bindings into static RDF context.

That is appropriate for this benchmark because the target is Janus segmented historical event-log access plus unified historical-live execution.

A future engine-level fix should preserve event multiplicity explicitly when evaluating aggregates over repeated event observations.

## Implementation notes and scope boundaries

- The accepted design note in `docs/decisions/2026-06-17-hybrid-scaling-combined.md` describes an earlier benchmark shape with a static queried historical window of `1,000` events across all `H`. The current implementation does not do that. The current implementation anchors all query types at the end of the historical log and varies the selected range by query type and `H`.
- The accepted query-types design note in `docs/decisions/2026-06-17-hybrid-scaling-query-types.md` mentions adding slight fractional offsets to guarantee quad uniqueness. The current implementation instead uses the deterministic congestion formulas shown above. This document reflects the implementation, not the earlier design note.
