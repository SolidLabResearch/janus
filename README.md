# Janus

[![CI/CD Pipeline](https://github.com/yourusername/janus/workflows/CI%2FCD%20Pipeline/badge.svg)](https://github.com/yourusername/janus/actions)
[![Crates.io](https://img.shields.io/crates/v/janus.svg)](https://crates.io/crates/janus)
[![Documentation](https://docs.rs/janus/badge.svg)](https://docs.rs/janus)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

Janus is a hybrid engine for unified Live and Historical RDF Stream Processing, implemented in Rust.

The name "Janus" is inspired by the Roman deity Janus who is the guardian of doorways and transitions, and looks towards both the past and the future simultaneously. This dual perspective reflects Janus's capability to process both Historical and Live RDF streams in a unified manner utilizing a single query language and engine.

## Features

- 🔄 **Unified Processing**: Process both historical and live RDF streams seamlessly
- 🚀 **High Performance**: Built with Rust for maximum performance and safety
- 🔌 **Multiple Store Support**: Integration with various RDF stores (Oxigraph, Apache Jena, and more)
- 📊 **Stream Processing**: Real-time RDF stream processing capabilities
- 🔍 **RSP-QL Support**: Extended query language for stream and historical data
- 🛡️ **Type Safety**: Leverages Rust's type system for correctness

## Installation

### As a Library

Add this to your `Cargo.toml`:

```toml
[dependencies]
janus = "0.1.0"
```

### As a Command-Line Tool

```bash
cargo install janus
```

### From Source

```bash
git clone https://github.com/yourusername/janus.git
cd janus
cargo build --release
```

## Quick Start

### Using as a Library

```rust
use janus::{Error, Result};

fn main() -> Result<()> {
    println!("Janus RDF Stream Processing Engine");
    
    // TODO: Initialize engine
    // TODO: Connect to RDF store
    // TODO: Process streams
    // TODO: Execute queries
    
    Ok(())
}
```

### Running Examples

```bash
# Run the basic example
cargo run --example basic

# Run with verbose output
RUST_LOG=debug cargo run --example basic
```

## Architecture

Janus is designed with modularity and extensibility in mind:

- **Core Engine**: Main processing logic and coordination
- **Store Adapters**: Pluggable interfaces for different RDF stores
- **Stream Processors**: Real-time stream processing capabilities
- **Query Engine**: RSP-QL parser and executor
- **Configuration**: Flexible configuration management

For detailed architecture information, see [ARCHITECTURE.md](ARCHITECTURE.md).

## Development

### Prerequisites

- Rust 1.70.0 or later
- Cargo (comes with Rust)
- Docker (optional, for integration tests)

### Building

```bash
# Debug build
cargo build

# Release build
cargo build --release

# With all features
cargo build --all-features
```

### Testing

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run integration tests
cargo test --test '*'

# Run with coverage
cargo llvm-cov --html
```

### Linting and Formatting

```bash
# Format code
cargo fmt

# Run clippy
cargo clippy --all-targets --all-features

# Check formatting
cargo fmt --all -- --check
```

### Running with Docker Services

Start RDF store services for testing:

```bash
# Start Oxigraph
docker run -d -p 7878:7878 --name oxigraph-server oxigraph/oxigraph

# Start Apache Jena Fuseki
docker run -d -p 3030:3030 --platform linux/amd64 \
  -v $(pwd)/fuseki-config:/fuseki/configuration \
  -v $(pwd)/fuseki-config/shiro.ini:/fuseki/shiro.ini \
  --name jena-server stain/jena-fuseki
```

## Roadmap

- [x] Project structure and basic setup
- [ ] Core engine implementation
- [ ] Support for Oxigraph RDF store
- [ ] Support for Apache Jena Fuseki
- [ ] Basic stream processing capabilities
- [ ] RSP-QL query parser
- [ ] Integration with Kafka streams
- [ ] Integration with MQTT streams
- [ ] Support for additional RDF stores (Virtuoso, Blazegraph, etc.)
- [ ] Advanced query optimization
- [ ] Distributed processing support
- [ ] Web interface and REST API
- [ ] Performance benchmarks and optimization

## Contributing

We welcome contributions! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

### Development Workflow

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Make your changes
4. Run tests and linting
5. Commit your changes (`git commit -m 'feat: add amazing feature'`)
6. Push to the branch (`git push origin feature/amazing-feature`)
7. Open a Pull Request

## Documentation

- [API Documentation](https://docs.rs/janus)
- [Architecture Guide](ARCHITECTURE.md)
- [Contributing Guidelines](CONTRIBUTING.md)
- [Examples](examples/)

## Performance

Janus is designed for high-performance stream processing. Benchmarks are available in the `benches/` directory:

```bash
cargo bench
```

Results are stored in `target/criterion/` with detailed HTML reports.

## License

This project is licensed by [Ghent University - imec](https://www.ugent.be/ea/idlab/en) under the MIT License - see the [LICENSE.md](LICENCE.md) file for details.

## Citation

If you use Janus in your research, please cite:

```bibtex
@software{janus,
  title = {Janus: A Hybrid Engine for Unified Live and Historical RDF Stream Processing},
  author = {Bisen, Kush},
  year = {2024},
  url = {https://github.com/yourusername/janus}
}
```

## Contact

For any questions or inquiries, please contact:

- **Kush Bisen** - [mailkushbisen@gmail.com](mailto:mailkushbisen@gmail.com)
- **Issues** - [GitHub Issues](https://github.com/yourusername/janus/issues)

## Acknowledgments

- Ghent University - imec for supporting this project
- The Rust community for excellent tools and libraries
- Contributors and users of the Janus project

## Related Projects

- [Oxigraph](https://github.com/oxigraph/oxigraph) - Fast RDF database
- [Apache Jena](https://jena.apache.org/) - RDF framework and SPARQL engine
- [RSP-QL](https://streamreasoning.org/RSP-QL/) - RDF Stream Processing Query Language

---