# Janus

Janus is a Rust engine for hybrid RDF stream processing. It combines:

- historical query execution over locally stored RDF events
- live query execution over streaming RDF data
- a JanusQL parser that lowers hybrid queries to SPARQL and RSP-QL
- an HTTP/WebSocket API for query management and result streaming

## Repository Status

The backend repository is active and locally healthy:

- `cargo test --all-features` passes
- `cargo clippy --all-targets --all-features -- -D warnings` passes
- the HTTP API, Janus API, parser, storage layer, and stream bus all have integration coverage

This repository is the backend and engine implementation.

The maintained dashboard lives in a separate repository:
- `https://github.com/SolidLabResearch/janus-dashboard`

The `janus-dashboard/` folder in this repository is a lightweight local demo client, not the primary frontend.

## What You Can Run

### HTTP API server

```bash
cargo run --bin http_server
```

This starts the backend on `http://127.0.0.1:8080` by default.

### Stream replay / ingestion CLI

```bash
cargo run --bin stream_bus_cli -- --help
```

Use this to replay RDF input files into storage and optionally publish them to MQTT.

### Tests

```bash
make test
```

### Linting

```bash
make lint
```

## Development

### Prerequisites

- Rust stable
- Cargo
- Docker, if you want to run the local MQTT broker from `docker-compose.yml`

### Build

```bash
make build
make release
```

### Full local CI checks

```bash
make ci-check
```

This runs formatting, clippy, tests, and build checks.

## Documentation

Start here:

- [GETTING_STARTED.md](./GETTING_STARTED.md)
- [START_HERE.md](./START_HERE.md)
- [docs/README.md](./docs/README.md)
- [docs/README_HTTP_API.md](./docs/README_HTTP_API.md)

Performance notes:

- [docs/BENCHMARK_RESULTS.md](./docs/BENCHMARK_RESULTS.md)

## Notes

- `src/main.rs` is currently a benchmark-style executable, not the main user-facing interface.
- The primary user-facing entry point is `http_server`.

## Licence

This code is copyrighted by Ghent University - imec and released under the MIT Licence.

## Contact

For questions, contact [Kush](mailto:mailkushbisen@gmail.com) or open an issue in the repository.
