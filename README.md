# Janus

Janus is a Rust engine for unified historical and live RDF stream processing.
It stores RDF events in a segmented, dictionary-encoded event log, evaluates
historical windows over that log, and evaluates live windows over MQTT-backed
streams. A Janus-QL query can combine both paths and deliver results through a
REST and WebSocket API.

The name refers to the Roman deity associated with looking backward and
forward: Janus is built to query past and arriving RDF data together.

## Capabilities

- Fixed and sliding historical windows over persisted RDF events
- Sliding live windows over RDF streams
- Hybrid queries containing both `ON LOG` and `ON STREAM` windows
- Janus-QL parsing, validation, and lowering to historical SPARQL and live
  RSP-QL execution paths
- Historical materialization through nested historical subqueries
- Query lifecycle management and result delivery through HTTP and WebSockets
- MQTT replay and ingestion through the stream-bus CLI or HTTP replay API
- Dictionary encoding, segmented storage, and persistent storage-footprint
  benchmarking

Janus also contains implementation support for baseline-backed hybrid queries
and anomaly-oriented extension functions. See [Janus-QL](./docs/JANUSQL.md) and
[the baseline guide](./docs/BASELINES.md) for their supported shapes and
limitations.

## Query model

Janus-QL declares each data source as a named window:

- `ON LOG` reads the persisted historical event log. Use `[START … END …]` for
  a fixed range, or `[OFFSET … RANGE … STEP …]` for a historical sliding
  window.
- `ON STREAM` reads a live stream. Use `[RANGE … STEP …]` for a live sliding
  window.

For example, this query combines a recent live window with a fixed historical
window:

```sparql
PREFIX ex: <http://example.org/>

REGISTER RStream ex:out AS
SELECT ?sensor ?reading ?reference
FROM NAMED WINDOW ex:live ON STREAM ex:sensors [RANGE 60000 STEP 1000]
FROM NAMED WINDOW ex:history ON LOG ex:sensors [START 1700000000000 END 1700086400000]
WHERE {
  WINDOW ex:live {
    ?sensor ex:hasReading ?reading .
  }
  WINDOW ex:history {
    ?sensor ex:referenceValue ?reference .
  }
}
```

The parser validates the relationship between a window source and its bounds;
for example, `START`/`END` belongs to a log window, while `RANGE`/`STEP`
belongs to a stream window. The complete syntax and execution details are in
[docs/JANUSQL.md](./docs/JANUSQL.md) and
[docs/QUERY_EXECUTION.md](./docs/QUERY_EXECUTION.md).

## Quick start

### Prerequisites

- A current stable Rust toolchain with Cargo
- Docker Compose, only for MQTT-backed replay or live-query flows

Build and run the test suite:

```bash
make build
make test
```

Start a local MQTT broker when exercising live streaming:

```bash
docker-compose up -d mosquitto
```

Start the HTTP and WebSocket API in another terminal:

```bash
cargo run --bin http_server -- \
  --host 127.0.0.1 \
  --port 8080 \
  --storage-dir ./data/storage
```

Confirm that the server is running:

```bash
curl http://127.0.0.1:8080/health
```

The fastest local API exercise is the client example:

```bash
cargo run --example http_client_example
```

It covers registering, starting, inspecting, and stopping a query; replay
control; and consuming results over WebSockets. For request and response
formats, use [the current HTTP API reference](./docs/HTTP_API_CURRENT.md).

## Entry points

| Command | Purpose |
| --- | --- |
| `cargo run --bin http_server -- --help` | Run the REST/WebSocket server. |
| `cargo run --bin stream_bus_cli -- --help` | Replay N-Triples or N-Quads to storage and, optionally, MQTT. |
| `cargo run --bin janus -- info` | Show the package-level entry points. |
| `cargo run --example http_client_example` | Exercise the HTTP and WebSocket query lifecycle. |
| `cargo bench --no-run` | Compile every Criterion benchmark without executing it. |

The repository also contains focused paper and storage benchmark binaries under
`src/bin/`. Their commands, output contracts, and reproducibility rules are in
[docs/BENCHMARKING.md](./docs/BENCHMARKING.md) and
[docs/PAPER_BENCHMARKING.md](./docs/PAPER_BENCHMARKING.md). Benchmark results
are workload- and machine-dependent; do not treat historical numbers as a
current performance guarantee.

## Development

```bash
make build         # debug build
make release       # optimized build
make test          # cargo test --all-features
make fmt           # format code
make fmt-check     # check formatting
make lint          # clippy, with warnings denied
make check         # formatting and linting
make ci-check      # local CI script
make doc-links     # validate local Markdown links
```

The main implementation areas are:

- `src/parsing/` — Janus-QL parsing and validation
- `src/api/` and `src/http/` — query lifecycle and REST/WebSocket interfaces
- `src/execution/` and `src/stream/` — historical and live execution
- `src/storage/` — segmented event storage and indexes
- `src/stream_bus/` — replay and MQTT publishing
- `src/bin/`, `benches/`, and `examples/` — runnable tools, benchmarks, and
  examples

## Documentation

Begin with [GETTING_STARTED.md](./GETTING_STARTED.md) or
[START_HERE.md](./START_HERE.md). The complete reading order and guide index
are in [docs/DOCUMENTATION_INDEX.md](./docs/DOCUMENTATION_INDEX.md).

Useful references:

- [Janus-QL](./docs/JANUSQL.md)
- [Current HTTP API](./docs/HTTP_API_CURRENT.md)
- [Live streaming guide](./docs/LIVE_STREAMING_GUIDE.md)
- [Stream-bus CLI](./docs/STREAM_BUS_CLI.md)
- [Benchmarking](./docs/BENCHMARKING.md)
- [Paper artifact map](./docs/PAPER_ARTIFACT_MAP.md)

The maintained web dashboard is a separate project:
[SolidLabResearch/janus-dashboard](https://github.com/SolidLabResearch/janus-dashboard).

## Contributing and licence

See [CONTRIBUTING.md](./CONTRIBUTING.md) for local development and contribution
guidance. Janus is copyrighted by Ghent University — imec and released under
the [MIT License](./LICENCE.md).

## Contact

For questions, contact [Kush](mailto:mailkushbisen@gmail.com) or open an issue
in the repository.
