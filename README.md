# Janus

Janus is a hybrid engine for unified Live and Historical RDF Stream Processing.

The name "Janus" is inspired by the Roman deity Janus who is the guardian of doorways and transitions, and looks towards both the past and the future simultaneously. This dual perspective reflects Janus's capability to process both Historical and Live RDF streams in a unified manner utilizing a single query language and engine.

## Prerequisites

- Node.js >= 18.0.0
- npm >= 9.0.0
- Rust >= 1.70.0
- wasm-pack (for WASM compilation)

### Installing Prerequisites

```bash
# Install Node.js (using nvm)
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.0/install.sh | bash
nvm install 18
nvm use 18

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install wasm-pack
curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
```

## Installation

```bash
# Clone the repository
git clone https://github.com/yourusername/janus.git
cd janus

# Install Node dependencies
npm install

# Build Rust library and WASM modules
npm run build:rust

# Build TypeScript
npm run build:ts

# Or build everything
npm run build
```
## Configuration

### Environment Variables

Create a `.env` file in the project root:

```env
# Oxigraph endpoint
OXIGRAPH_ENDPOINT=http://localhost:7878

# Jena Fuseki endpoint
JENA_ENDPOINT=http://localhost:3030
JENA_DATASET=myDataset
JENA_AUTH_TOKEN=your-token-here

# Logging
LOG_LEVEL=info

# WASM
ENABLE_WASM=true
```
## Building for Production

```bash
# Build optimized production bundle
npm run build

# Build TypeScript only
npm run build:ts

# Build Rust library
npm run build:rust

# Build WASM for web
npm run build:rust:wasm

# Clean build artifacts
npm run clean
```

## Deployment

### NPM Package

```bash
# Prepare for publishing
npm run prepublishOnly

# Publish to npm
npm publish
```

### Docker

Create a `Dockerfile`:

```dockerfile
FROM node:18-alpine

WORKDIR /app

# Install Rust
RUN apk add --no-cache curl gcc musl-dev
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# Install wasm-pack
RUN curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh

# Copy package files
COPY package*.json ./
COPY rust/ ./rust/

# Install dependencies
RUN npm ci

# Build project
RUN npm run build

# Copy source
COPY . .

# Start application
CMD ["npm", "start"]
```

## Contributing

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/my-feature`
3. Commit your changes: `git commit -am 'Add new feature'`
4. Push to the branch: `git push origin feature/my-feature`
5. Submit a pull request

### Development Workflow

```bash
# Create a new branch
git checkout -b feature/my-feature

# Make changes and test
npm run dev
npm test

# Lint and format
npm run lint
npm run format

# Commit with conventional commits
git commit -m "feat: add new RDF adapter"

# Push and create PR
git push origin feature/my-feature
```

## API Documentation

### Core Types

- `RdfFormat`: Enum of supported RDF formats
- `QueryResultFormat`: Enum of SPARQL result formats
- `RdfTerm`: Interface for RDF terms (URI, Literal, Blank Node)
- `RdfTriple`: Interface for RDF triples/quads
- `QueryResult`: Union type for query results

### Adapters

- `OxigraphAdapter`: HTTP adapter for Oxigraph
- `JenaAdapter`: HTTP adapter for Apache Jena Fuseki
- `WasmAdapter`: Direct WASM integration

### Utilities

- `Logger`: Structured logging with levels
- `ErrorHandler`: Error handling and retry logic
- `RdfError`: Custom error types for RDF operations

## Troubleshooting

### WASM Build Fails

```bash
# Install wasm-pack
curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh

# Add wasm32 target
rustup target add wasm32-unknown-unknown

# Clean and rebuild
npm run clean
npm run build:rust
```

### TypeScript Compilation Errors

```bash
# Clean node_modules and reinstall
rm -rf node_modules package-lock.json
npm install

# Rebuild
npm run build:ts
```

### Rust Linking Errors

```bash
# Update Rust
rustup update stable

# Clean Cargo cache
cd rust && cargo clean

# Rebuild
cargo build --release
```

## License

This project is licensed by Ghent University - imec under the MIT License - see the [LICENSE.md](LICENSE.md) file for details.

## Roadmap

- [ ] Support for additional RDF stores (e.g., Virtuoso, Blazegraph, Kolibrie etc)
- [ ] Support for RDF Stream Processing Engine.
- [ ] Exposing an interface for the RDF Stream Processing Engine and integration with Historical Data.
- [ ] Extending RSP-QL to support querying both Historical and Live RDF streams seamlessly and implementing the necessary engine capabilities and extending the parser.