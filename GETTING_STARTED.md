# Getting Started with Janus

Janus is a Rust engine for querying historical and live RDF data through one
Janus-QL model and one HTTP/WebSocket API.

## Prerequisites

- Rust stable with Cargo
- Docker and Docker Compose if you want the MQTT-backed replay flow

## Fastest Working Path

### 1. Build and test

```bash
make build
make test
```

### 2. Start the HTTP server

```bash
cargo run --bin http_server -- --host 127.0.0.1 --port 8080 --storage-dir ./data/storage
```

Verify it is up:

```bash
curl http://127.0.0.1:8080/health
```

### 3. Exercise the API

The quickest end-to-end client is the example binary:

```bash
cargo run --example http_client_example
```

That example covers query registration, start, stop, replay control, and
WebSocket result consumption.

## Optional Local Demo UI

This repository keeps a small static demo at
`examples/demo_dashboard.html` for manual browser testing.

The maintained Svelte dashboard lives in the separate
`SolidLabResearch/janus-dashboard` repository.

## Main Binaries

- `http_server`: REST and WebSocket API for query lifecycle and replay control
- `stream_bus_cli`: replay and ingestion CLI for RDF event files

## Common Commands

```bash
make build
make release
make test
make fmt
make fmt-check
make lint
make check
make ci-check
```

## Repository Layout

- `src/api`: query lifecycle orchestration
- `src/http`: REST and WebSocket server
- `src/parsing`: Janus-QL parsing
- `src/execution`: historical execution
- `src/stream`: live stream processing
- `src/storage`: segmented RDF storage
- `src/bin`: executable binaries
- `examples`: runnable examples and a minimal static demo
- `tests`: integration coverage
- `docs`: current docs plus older design notes

## Where to Read Next

- `README.md`
- `START_HERE.md`
- `docs/DOCUMENTATION_INDEX.md`

## Common Rust Commands

Here are some useful Cargo commands:

```bash
# Build the project
cargo build

# Build with optimizations
cargo build --release

# Run the binary
cargo run

# Run with arguments
cargo run -- --help

# Run tests
cargo test

# Run a specific test
cargo test test_name

# Run examples
cargo run --example basic

# Generate documentation
cargo doc --open

# Check code without building
cargo check

# Update dependencies
cargo update

# Show dependency tree
cargo tree

# Clean build artifacts
cargo clean
```

## Using Rust Analyzer

For the best development experience, install Rust Analyzer in your editor:

- **VS Code**: Install the "rust-analyzer" extension
- **IntelliJ/CLion**: Rust plugin comes with IntelliJ Rust
- **Vim/Neovim**: Use rust-analyzer with LSP plugins
- **Emacs**: Use lsp-mode with rust-analyzer

## Next Steps

Now that you have Janus set up, here's what you can do next:

1. **Explore the Architecture**: Read [ARCHITECTURE.md](ARCHITECTURE.md) to understand the design
2. **Read the Contributing Guide**: Check [CONTRIBUTING.md](CONTRIBUTING.md) for contribution guidelines
3. **Start Implementing**: Begin with one of the TODO items in the roadmap
4. **Write Tests**: Add tests for any new functionality
5. **Improve Documentation**: Help improve docs and examples

## Common Issues

### Issue: Rust not found

**Solution**: Ensure Rust is installed and in your PATH. Restart your terminal after installation.

### Issue: Docker containers fail to start

**Solution**: Check if ports 7878 and 3030 are already in use:

```bash
lsof -i :7878
lsof -i :3030
```

### Issue: Tests failing

**Solution**: Make sure all dependencies are up to date:

```bash
cargo update
cargo build
cargo test
```

### Issue: Compilation errors

**Solution**: Clean the build and rebuild:

```bash
cargo clean
cargo build
```

## Learning Resources

### Rust

- [The Rust Book](https://doc.rust-lang.org/book/)
- [Rust by Example](https://doc.rust-lang.org/rust-by-example/)
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)

### RDF and SPARQL

- [RDF Primer](https://www.w3.org/TR/rdf11-primer/)
- [SPARQL Tutorial](https://www.w3.org/TR/sparql11-query/)
- [RSP-QL Specification](https://streamreasoning.org/RSP-QL/)

### Stream Processing

- [Stream Processing Concepts](https://www.oreilly.com/library/view/streaming-systems/9781491983867/)
- [RDF Stream Processing](https://streamreasoning.org/)

## Getting Help

If you need help:

1. **Check the documentation**: Read the existing docs and code comments
2. **Search issues**: Look for similar issues on GitHub
3. **Open an issue**: Create a new issue with details about your problem
4. **Contact maintainers**: Email [mailkushbisen@gmail.com](mailto:mailkushbisen@gmail.com)

## Contributing

We welcome contributions! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for:

- How to submit pull requests
- Coding standards
- Testing requirements
- Review process

## License

This project is licensed under the MIT License - see [LICENCE.md](LICENCE.md) for details.

---

Happy coding with Janus!
