# Getting Started with Janus

Welcome to Janus! This guide will help you get up and running with the Janus RDF Stream Processing Engine.

## What is Janus?

Janus is a hybrid engine for unified Live and Historical RDF Stream Processing, written in Rust. It allows you to seamlessly process both historical RDF data stored in databases and live RDF streams in real-time using a single query language.

## Prerequisites

Before you begin, ensure you have the following installed:

- **Rust** (1.70.0 or later) - [Install from rustup.rs](https://rustup.rs/)
- **Cargo** (comes with Rust)
- **Git** (for cloning the repository)
- **Docker** (optional, for running RDF stores)

### Installing Rust

If you don't have Rust installed, run:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

After installation, restart your terminal and verify:

```bash
rustc --version
cargo --version
```

## Quick Start

### 1. Clone the Repository

```bash
git clone https://github.com/yourusername/janus.git
cd janus
```

### 2. Build the Project

```bash
# Debug build (faster compilation, slower execution)
cargo build

# Release build (slower compilation, faster execution)
cargo build --release
```

### 3. Run Tests

Verify everything is working correctly:

```bash
cargo test
```

### 4. Run the Example

```bash
cargo run --example basic
```

You should see output explaining the steps Janus takes to process RDF streams.

### 5. Run the CLI Tool

```bash
cargo run
```

This will display the version and basic information about Janus.

## Project Structure

Understanding the project structure will help you navigate the codebase:

```
janus/
├── src/                    # Source code
│   ├── lib.rs             # Library entry point
│   ├── main.rs            # Binary entry point
│   ├── core/              # Core engine logic (to be implemented)
│   ├── store/             # RDF store adapters (to be implemented)
│   ├── stream/            # Stream processing (to be implemented)
│   ├── query/             # Query engine (to be implemented)
│   └── config/            # Configuration (to be implemented)
├── examples/              # Usage examples
│   └── basic.rs           # Basic example
├── tests/                 # Integration tests
│   └── integration_test.rs
├── benches/               # Performance benchmarks
├── fuseki-config/         # Apache Jena Fuseki configuration
├── Cargo.toml             # Project metadata and dependencies
├── Makefile               # Common development tasks
└── README.md              # Project overview
```

## Using the Makefile

The project includes a `Makefile` with common development tasks:

```bash
# See all available commands
make help

# Build the project
make build

# Run tests
make test

# Format code
make fmt

# Run linter
make lint

# Run all checks
make check

# Generate documentation
make doc

# Run benchmarks
make bench

# Start Docker services (Oxigraph + Jena)
make docker-start

# Stop Docker services
make docker-stop
```

## Development Workflow

### 1. Set Up Your Development Environment

```bash
# Install development tools
make setup

# Verify everything is installed
make setup-check
```

### 2. Make Changes

Edit files in the `src/` directory. The main areas to implement are:

- `src/core/` - Core engine logic
- `src/store/` - RDF store adapters
- `src/stream/` - Stream processing
- `src/query/` - Query parsing and execution

### 3. Test Your Changes

```bash
# Run tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_name
```

### 4. Format and Lint

```bash
# Format code
cargo fmt

# Check formatting
cargo fmt --check

# Run linter
cargo clippy
```

### 5. Build and Run

```bash
# Build
cargo build

# Run
cargo run

# Run example
cargo run --example basic
```

## Working with RDF Stores

Janus is designed to work with multiple RDF stores. Here's how to set them up for development:

### Oxigraph

Start Oxigraph using Docker:

```bash
docker run -d -p 7878:7878 --name oxigraph-server oxigraph/oxigraph
```

Or use the Makefile:

```bash
make docker-oxigraph
```

Oxigraph will be available at `http://localhost:7878`

### Apache Jena Fuseki

Start Jena Fuseki using Docker:

```bash
docker run -d -p 3030:3030 --platform linux/amd64 \
  -v $(pwd)/fuseki-config:/fuseki/configuration \
  -v $(pwd)/fuseki-config/shiro.ini:/fuseki/shiro.ini \
  --name jena-server stain/jena-fuseki
```

Or use the Makefile:

```bash
make docker-jena
```

Fuseki will be available at `http://localhost:3030`

### Starting Both Services

```bash
make docker-start
```

### Stopping Services

```bash
make docker-stop
```

## Adding Dependencies

To add a new dependency, edit `Cargo.toml`:

```toml
[dependencies]
# Add your dependency here
tokio = { version = "1.35", features = ["full"] }
```

Then run:

```bash
cargo build
```

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

Happy coding with Janus! 🚀