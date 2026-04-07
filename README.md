# Janus

Janus is a hybrid engine for unified Live and Historical RDF Stream Processing, implemented in Rust.

The name "Janus" is inspired by the Roman deity Janus who is the guardian of doorways and transitions, and looks towards both the past and the future simultaneously. This dual perspective reflects Janus's capability to process both Historical and Live RDF streams in a unified manner utilizing a single query language and engine.

## Performance

Janus achieves high-throughput RDF stream processing with dictionary encoding and streaming segmented storage:

- Write Throughput: 2.6-3.14 Million quads/sec
- Read Throughput: 2.7-2.77 Million quads/sec
- Point Query Latency: Sub-millisecond (0.235 ms at 1M quads)
- Space Efficiency: 40% reduction through dictionary encoding (24 bytes vs 40 bytes per event)

For detailed benchmark results, see [BENCHMARK_RESULTS.md](./BENCHMARK_RESULTS.md).

## Development

### Prerequisites

- Rust (stable toolchain)
- Cargo

### Building

```bash
# Debug build
make build

# Release build (optimized)
make release
```

### Testing

```bash
# Run all tests
make test

# Run tests with verbose output
make test-verbose
```

### Code Quality

Before pushing to the repository, run the CI/CD checks locally:

```bash
# Run all CI/CD checks (formatting, linting, tests, build)
make ci-check

# Or use the script directly
   ./scripts/ci-check.sh```

This will run:
- **rustfmt** - Code formatting check
- **clippy** - Lint checks with warnings as errors
- **tests** - Full test suite
- **build** - Compilation check

Individual checks can also be run:

```bash
make fmt        # Format code
make fmt-check  # Check formatting
make lint       # Run Clippy
make check      # Run formatting and linting checks
```

## Licence 

This code is copyrighted by Ghent University - imec and released under the MIT Licence

## Contact

For any questions, please contact [Kush](mailto:mailkushbisen@gmail.com) or create an issue in the repository.

---
