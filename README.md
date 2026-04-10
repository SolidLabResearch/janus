# Janus

Janus is a Rust engine for unified historical and live RDF stream processing.

It combines:

- historical window evaluation over segmented RDF storage
- live window evaluation over incoming streams
- a single Janus-QL query model for hybrid queries
- an HTTP/WebSocket API for query lifecycle management and result delivery

The name comes from the Roman deity Janus, associated with transitions and with looking both backward and forward. That dual perspective matches Janus's goal: querying past and live RDF data together.

## What Janus Supports

- Historical windows with `START` / `END`
- Sliding live windows with `RANGE` / `STEP`
- Hybrid queries that mix historical and live windows
- Extension functions for anomaly-style predicates such as thresholds, relative change, z-score, outlier checks, and trend divergence
- Optional baseline bootstrapping for hybrid anomaly queries with `USING BASELINE <window> LAST|AGGREGATE`
- HTTP endpoints for registering, starting, stopping, listing, and deleting queries
- WebSocket result streaming for running queries

## Query Model

Janus uses Janus-QL, a hybrid query language for querying historical and live RDF data in one query.

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

`USING BASELINE` is optional. If present, Janus bootstraps baseline values from the named historical window before or during live execution:

- `LAST`: use the final historical window snapshot as baseline
- `AGGREGATE`: merge the historical window outputs into one compact baseline

## Repository Status

The backend repository is active and locally healthy:

- `cargo test --all-features` passes
- `cargo clippy --all-targets --all-features -- -D warnings` passes
- the HTTP API, Janus API, parser, storage layer, and stream bus all have integration coverage

This repository is the backend and engine implementation.

The maintained dashboard lives in a separate repository:

- `https://github.com/SolidLabResearch/janus-dashboard`

## Performance

Janus uses dictionary encoding and segmented storage for high-throughput ingestion and historical reads.

- Write throughput: 2.6-3.14 million quads/sec
- Read throughput: 2.7-2.77 million quads/sec
- Point query latency: 0.235 ms at 1M quads
- Space efficiency: about 40% smaller encoded events

Detailed benchmark data is in [docs/BENCHMARK_RESULTS.md](./docs/BENCHMARK_RESULTS.md).

## Quick Start

### Prerequisites

- Rust stable
- Cargo
- Docker, if you want to run the local MQTT broker from `docker-compose.yml`

### Build

```bash
make build
make release
```

### Run the HTTP API

```bash
cargo run --bin http_server -- --host 127.0.0.1 --port 8080 --storage-dir ./data/storage
```

Then check the server:

```bash
curl http://127.0.0.1:8080/health
```

### Try the HTTP client example

```bash
cargo run --example http_client_example
```

This example demonstrates:

- query registration
- query start and stop
- query inspection
- replay control
- WebSocket result consumption

### Frontend Boundary

The maintained web dashboard lives in the separate
`SolidLabResearch/janus-dashboard` repository.

Frontend development should happen in the dedicated dashboard repo.

## Development

### Common Commands

```bash
make build         # debug build
make release       # optimized build
make test          # full test suite
make test-verbose  # verbose tests
make fmt           # format code
make fmt-check     # check formatting
make lint          # clippy with warnings as errors
make check         # formatting + linting
make ci-check      # local CI script
```

### Examples

The repository includes runnable examples under [`examples/`](./examples), including:

- [`examples/http_client_example.rs`](./examples/http_client_example.rs)
- [`examples/comparator_demo.rs`](./examples/comparator_demo.rs)

## Documentation

Start here:

- [GETTING_STARTED.md](./GETTING_STARTED.md)
- [START_HERE.md](./START_HERE.md)
- [docs/DOCUMENTATION_INDEX.md](./docs/DOCUMENTATION_INDEX.md)
- [docs/README.md](./docs/README.md)
- [docs/HTTP_API_CURRENT.md](./docs/HTTP_API_CURRENT.md)

## Notes

- `src/main.rs` is now a lightweight entry binary that points to the main Janus
  executables and benchmark helpers.
- The primary user-facing entry point is `http_server`.

## Licence

This code is copyrighted by Ghent University - imec and released under the MIT Licence.

## Contact

For questions, contact [Kush](mailto:mailkushbisen@gmail.com) or open an issue in the repository.
