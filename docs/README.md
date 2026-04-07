# Janus Documentation

This directory contains comprehensive documentation for the Janus RDF Stream Processing engine.

## Core Documentation

### Architecture & Design
- **[ARCHITECTURE.md](ARCHITECTURE.md)** - High-level system architecture and design principles
- **[MVP_ARCHITECTURE.md](MVP_ARCHITECTURE.md)** - Minimum Viable Product architecture details
- **[RSP_INTEGRATION_COMPLETE.md](RSP_INTEGRATION_COMPLETE.md)** - RSP-RS integration documentation

### Performance & Benchmarking
- **[BENCHMARK_RESULTS.md](BENCHMARK_RESULTS.md)** - Performance metrics and benchmark results
- **[WRITING_BENCHMARKS.md](WRITING_BENCHMARKS.md)** - Guide for writing performance benchmarks

### Features & Components
- **[STREAM_BUS_CLI.md](STREAM_BUS_CLI.md)** - Command-line interface documentation
- **[SPARQL_BINDINGS_UPGRADE.md](SPARQL_BINDINGS_UPGRADE.md)** - SPARQL structured bindings feature
- **[EXECUTION_ARCHITECTURE.md](EXECUTION_ARCHITECTURE.md)** - ✨ Query execution architecture (NEW)

### Getting Started
- **[MVP_QUICKSTART.md](MVP_QUICKSTART.md)** - Quick start guide for MVP features
- **[MVP_TODO.md](MVP_TODO.md)** - Current development roadmap and TODOs

## Recent Updates

### Execution Architecture (Latest)
Built internal execution layer for historical and live query processing:
- `HistoricalExecutor` for querying historical RDF data with SPARQL
- `ResultConverter` for unified result formatting
- Supports both fixed and sliding windows
- Thread-safe with message passing architecture
- 12 comprehensive unit tests
- See [EXECUTION_ARCHITECTURE.md](EXECUTION_ARCHITECTURE.md) for details

### SPARQL Structured Bindings
Enhanced `OxigraphAdapter` with `execute_query_bindings()` method for structured SPARQL results:
- Returns `Vec<HashMap<String, String>>` instead of debug format strings
- 12 comprehensive tests covering all query types
- Full backward compatibility maintained
- See [SPARQL_BINDINGS_UPGRADE.md](SPARQL_BINDINGS_UPGRADE.md) for details

## Quick Links

### Development
```bash
# Build project
make build

# Run tests
make test

# Format code
make fmt

# Run clippy
make clippy
```

### Testing
```bash
# Run all tests
cargo test

# Run specific test file
cargo test --test oxigraph_adapter_test

# Run with output
cargo test -- --nocapture
```

### Documentation
```bash
# Build and view docs
cargo doc --no-deps --open

# Check docs build
cargo doc --no-deps --package janus
```

## Project Structure

```
janus/
├── src/
│   ├── core/           # Core RDF event types and encoding
│   ├── storage/        # Storage engine and indexing
│   ├── execution/      # Query execution (historical + live)
│   ├── querying/       # SPARQL query processing (Oxigraph)
│   ├── parsing/        # JanusQL parser
│   ├── api/            # Public API layer
│   └── stream_bus/     # Event streaming infrastructure
├── tests/              # Integration tests
├── examples/           # Benchmark examples
└── docs/               # This directory
```

## Contributing

When adding new features:
1. Follow patterns in existing code
2. Add comprehensive tests (aim for >80% coverage)
3. Update relevant documentation
4. Run `make fmt` and `make clippy` before committing
5. Add changelog entry to this README if significant

## Support

For questions or issues:
- Check existing documentation first
- Review test files for usage examples
- See `.github/copilot-instructions.md` for coding standards