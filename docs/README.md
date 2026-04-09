# Janus Documentation

This directory mixes current guides with deeper implementation notes from earlier development phases. Start with the documents below; some older design and MVP notes are retained for historical context.

## Recommended Reading Order

1. [../README.md](../README.md)
2. [../GETTING_STARTED.md](../GETTING_STARTED.md)
3. [../START_HERE.md](../START_HERE.md)
4. [README_HTTP_API.md](README_HTTP_API.md)
5. [QUICKSTART_HTTP_API.md](QUICKSTART_HTTP_API.md)
6. [STREAM_BUS_CLI.md](STREAM_BUS_CLI.md)

## Current Guides

### Core usage

- [README_HTTP_API.md](README_HTTP_API.md) - current HTTP/WebSocket API guide
- [QUICKSTART_HTTP_API.md](QUICKSTART_HTTP_API.md) - short API quickstart
- [STREAM_BUS_CLI.md](STREAM_BUS_CLI.md) - replay and ingestion CLI
- [HTTP_API.md](HTTP_API.md) - API reference details

### Performance and architecture

- [BENCHMARK_RESULTS.md](BENCHMARK_RESULTS.md) - benchmark data and measurements
- [ARCHITECTURE.md](ARCHITECTURE.md) - high-level architecture
- [EXECUTION_ARCHITECTURE.md](EXECUTION_ARCHITECTURE.md) - historical/live execution details

## Historical / Planning Documents

These are useful for design context, but they should not be treated as the source of truth for current repository status:

- [MVP_TODO.md](MVP_TODO.md)
- [MVP_ARCHITECTURE.md](MVP_ARCHITECTURE.md)
- [RSP_INTEGRATION_COMPLETE.md](RSP_INTEGRATION_COMPLETE.md)
- [SPARQL_BINDINGS_UPGRADE.md](SPARQL_BINDINGS_UPGRADE.md)

## Repo Boundary

This repository is the Janus backend and engine implementation.

The maintained dashboard lives in:

- `https://github.com/SolidLabResearch/janus-dashboard`

The dashboard code checked into this repository should be treated as a local demo client unless stated otherwise.
