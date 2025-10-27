# Janus RDF Template - Architecture Documentation

Note: The WASM adapter has been fully implemented with Rust WASM integration for high-performance RDF processing.

## Overview

Janus is a hybrid TypeScript + Rust architecture template designed for efficient RDF (Resource Description Framework) data store integration. It provides a unified interface for interacting with multiple RDF triple stores through both HTTP APIs and WebAssembly (WASM) bindings.

## Design Principles

1. Hybrid Architecture: Leverage TypeScript for developer ergonomics and Rust for performance-critical operations
2. Adapter Pattern: Provide consistent interfaces for different RDF store implementations
3. Type Safety: Full TypeScript type definitions with strict typing enabled
4. Performance: Use WASM for low-latency, in-browser/server-side RDF processing
5. Testability: Comprehensive test coverage with both unit and integration tests
6. Extensibility: Easy to add new RDF store adapters and formats

## System Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     Application Layer (TypeScript)               │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐          │
│  │   Web App    │  │   CLI Tool   │  │  API Server  │          │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘          │
└─────────┼──────────────────┼──────────────────┼──────────────────┘
          │                  │                  │
┌─────────┼──────────────────┼──────────────────┼──────────────────┐
│         │         Janus RDF Framework (TypeScript)               │
│         │                  │                  │                  │
│  ┌──────▼──────────────────▼──────────────────▼─────┐           │
│  │           Core Types & Interfaces                 │           │
│  │  - RDF Terms, Triples, Query Results             │           │
│  │  - Store Configuration & Options                  │           │
│  └───────────────────────┬──────────────────────────┘           │
│                          │                                       │
│  ┌───────────────────────┴──────────────────────────┐           │
│  │              Adapter Layer                        │           │
│  │  ┌────────────┐  ┌────────────┐  ┌────────────┐ │           │
│  │  │ Oxigraph   │  │    Jena    │  │   WASM     │ │           │
│  │  │  Adapter   │  │  Adapter   │  │  Adapter   │ │           │
│  │  └─────┬──────┘  └─────┬──────┘  └─────┬──────┘ │           │
│  └────────┼───────────────┼───────────────┼────────┘           │
│           │               │               │                     │
│  ┌────────┼───────────────┼───────────────┼────────┐           │
│  │        │    Utilities Layer            │        │           │
│  │  ┌─────▼──────┐  ┌───────────┐  ┌─────▼──────┐ │           │
│  │  │   Logger   │  │  Errors   │  │ Validators │ │           │
│  │  └────────────┘  └───────────┘  └────────────┘ │           │
│  └───────────────────────────────────────────────┘            │
└─────────────┬───────────────────────────────┬──────────────────┘
              │ HTTP                          │ WASM Bindings
┌─────────────▼──────────────┐  ┌─────────────▼──────────────────┐
│  Rust RDF Library (WASM)   │  │    External RDF Stores         │
│  ┌──────────────────────┐  │  │  ┌──────────┐  ┌───────────┐  │
│  │  Oxigraph Store      │  │  │  │ Oxigraph │  │   Jena    │  │
│  │  Query Executor      │  │  │  │  Server  │  │  Fuseki   │  │
│  │  RDF Parser          │  │  │  │  (HTTP)  │  │  (HTTP)   │  │
│  │  HTTP Client         │  │  │  └──────────┘  └───────────┘  │
│  └──────────────────────┘  │  │                                │
└────────────────────────────┘  └────────────────────────────────┘
```

## Core Components

### 1. TypeScript Layer

#### Core Types & Interfaces (`src/core/types.ts`)

**Enums:**
- RdfFormat: Supported RDF serialization formats (Turtle, N-Triples, RDF/XML, JSON-LD, N-Quads, TriG)
- RdfTermType: RDF term types (URI, Literal, Blank Node, Triple)
- RdfErrorType: Error classification types
- LogLevel: Logging levels (Debug, Info, Warn, Error)

**Core Interfaces:**
- RdfTerm: RDF term representation
- RdfTriple: RDF triple/quad structure
- QueryResult: Union type for SPARQL query results (Select, Ask, Construct)
- QueryOptions: Query execution parameters
- RdfEndpointConfig: RDF store endpoint configuration
- IRdfStoreAdapter: Common interface for all RDF store adapters
  - `loadData()`: Load RDF data into store
  - `query()`: Execute SPARQL queries
  - `insert()`: Insert triples
  - `remove()`: Remove triples
  - `size()`: Get triple count
  - `clear()`: Clear all data
  - `export()`: Export data in various formats
  - `contains()`: Check if triple exists
- ILogger: Logging interface
- WasmRdfStore, WasmQueryExecutor, WasmRdfParser, WasmRdfSerializer: WASM binding interfaces

### 2. Adapter Implementations

#### OxigraphAdapter (`src/adapters/OxigraphAdapter.ts`)
- Connects to Oxigraph via HTTP
- Supports all SPARQL 1.1 query types
- Implements Graph Store Protocol (GSP)
- Features:
  - Connection pooling
  - Request timeout handling
  - Authentication support
  - Comprehensive error handling

#### JenaAdapter (`src/adapters/JenaAdapter.ts`)
- Connects to Apache Jena Fuseki
- Dataset-aware operations
- Named graph support
- Features:
  - Form-encoded query parameters (Jena-specific)
  - File upload support
  - Server statistics
  - Multi-graph management

#### WasmAdapter (`src/adapters/WasmAdapter.ts`)
- Direct integration with Rust WASM module
- In-memory or persistent storage
- No HTTP overhead
- Features:
  - Synchronous and asynchronous APIs
  - Browser and Node.js compatible
  - Embedded RDF store

### 3. Rust Layer

#### Core Library (`rust/src/lib.rs`)
- WASM bindings for TypeScript integration
- Configuration types
- Format enumerations
- Initialization functions

#### Store Implementation (`rust/src/store.rs`)
- Wraps Oxigraph's in-memory store
- CRUD operations for triples
- Query execution
- Import/export functionality
- Features:
  - Transaction support
  - Graph-aware operations
  - Efficient indexing

#### Query Executor (`rust/src/query.rs`)
- SPARQL query parsing and execution
- Query validation
- Query builder for programmatic query construction
- Support for:
  - SELECT queries
  - ASK queries
  - CONSTRUCT queries
  - DESCRIBE queries

#### Parser/Serializer (`rust/src/parser.rs`)
- Multi-format RDF parsing
- Multi-format serialization
- Format conversion
- Streaming support for large datasets

#### HTTP Client (`rust/src/http_client.rs`)
- HTTP client for remote RDF stores
- Store-specific endpoint handling
- Authentication support
- Batch operations

#### Error Handling (`rust/src/error.rs`)
- Comprehensive error types
- Error conversion from dependencies
- WASM-compatible error wrapper
- Ergonomic error propagation

## Data Flow

### Query Execution Flow

```
User Code
    │
    ▼
adapter.query(sparql)
    │
    ├─► [HTTP Adapter] ──► HTTP Request ──► Remote Store ──► HTTP Response
    │                                           (Jena/Oxigraph)
    │
    └─► [WASM Adapter] ──► WASM Call ──► Rust Store ──► WASM Response
                                          (In-memory)
```

### Data Loading Flow

```
User Provides RDF Data (Turtle/N-Triples/etc.)
    │
    ▼
adapter.loadData(data, format)
    │
    ├─► [HTTP Adapter]
    │       │
    │       ├─► Convert format to Content-Type header
    │       ├─► Send HTTP POST to /store endpoint
    │       └─► Handle response
    │
    └─► [WASM Adapter]
            │
            ├─► Pass to Rust parser
            ├─► Parse RDF triples
            ├─► Insert into in-memory store
            └─► Return triple count
```

## Technology Stack

### TypeScript/JavaScript
- Runtime: Node.js 18+
- Language: TypeScript 5.3+
- HTTP Client: Axios
- Testing: Jest
- Linting: ESLint
- Formatting: Prettier

### Rust
- Version: 1.70+
- RDF Library: Oxigraph 0.3
- WASM: wasm-pack, wasm-bindgen
- Serialization: serde, serde_json
- HTTP: reqwest (optional feature)
- Testing: Built-in test framework
- Linting: Clippy

## Key Design Patterns

### 1. Adapter Pattern
All RDF store integrations implement the `IRdfStoreAdapter` interface, allowing seamless switching between different backends.

```typescript
interface IRdfStoreAdapter {
  loadData(data: string, format: RdfFormat, graphName?: string): Promise<number>;
  query(sparql: string, options?: QueryOptions): Promise<QueryResult>;
  insert(triple: RdfTriple): Promise<void>;
  // ... other methods
}
```

### 2. Strategy Pattern
Different query execution strategies based on store type and query complexity.

### 3. Factory Pattern
Adapter creation with configuration:

```typescript
function createAdapter(config: RdfEndpointConfig): IRdfStoreAdapter {
  switch (config.storeType) {
    case 'oxigraph': return new OxigraphAdapter(config);
    case 'jena': return new JenaAdapter(config);
    default: throw new Error('Unknown store type');
  }
}
```

### 4. Facade Pattern
Simplified API over complex RDF operations:

```typescript
class RdfStore {
  constructor(private adapter: IRdfStoreAdapter) {}
  
  async loadTurtle(data: string): Promise<void> {
    await this.adapter.loadData(data, RdfFormat.Turtle);
  }
}
```

## Performance Considerations

### 1. WASM Performance
- Pros: Near-native speed, no serialization overhead
- Cons: Initial load time, memory usage
- Use Case: In-browser processing, low-latency queries

### 2. HTTP Performance
- Pros: Distributed architecture, scalability
- Cons: Network latency, serialization overhead
- Use Case: Large datasets, shared stores, persistence

### 3. Optimization Techniques
- Connection pooling
- Query result caching
- Streaming for large datasets
- Batch operations
- Lazy loading

## Security Architecture

### 1. Authentication
- Bearer token authentication
- API key support
- JWT integration points

### 2. Input Validation
- SPARQL injection prevention
- IRI validation
- Format validation
- Schema validation

### 3. Rate Limiting
- Per-endpoint rate limits
- Per-user quotas
- Configurable thresholds

### 4. Network Security
- HTTPS/TLS enforcement
- CORS configuration
- Request sanitization

## Error Handling Strategy

### Error Hierarchy
```
RdfError (base)
├── ParseError
├── QueryError
├── SerializationError
├── StoreError
├── HttpError
├── InvalidIriError
├── InvalidFormatError
├── ConfigError
└── NotFoundError
```

### Error Recovery
- Automatic retry with exponential backoff
- Circuit breaker pattern for external services
- Graceful degradation
- Comprehensive error logging

## Testing Strategy

### 1. Unit Tests
- Individual function testing
- Mock external dependencies
- TypeScript: Jest
- Rust: `#[test]` functions

### 2. Integration Tests
- End-to-end adapter testing
- Real RDF store connections
- Multi-format validation
- Query execution verification

### 3. Performance Tests
- Benchmark suite using Criterion (Rust)
- Load testing with large datasets
- Query performance profiling

### 4. Coverage Goals
- TypeScript: >70% coverage
- Rust: >70% coverage
- Critical paths: 100% coverage

## Deployment Architecture

### Deployment Options

1. NPM Package: Library for other applications
2. Docker Container: Standalone service
3. Kubernetes: Scalable microservice
4. Serverless: AWS Lambda, Google Cloud Functions
5. Browser Bundle: Client-side WASM application

### Scalability

#### Horizontal Scaling
```
Load Balancer
    ├── Janus Instance 1 ──► Oxigraph Cluster
    ├── Janus Instance 2 ──► Jena Fuseki Cluster
    └── Janus Instance N ──► Cache Layer
```

#### Vertical Scaling
- Increase memory for larger datasets
- More CPU cores for parallel query processing
- SSD storage for faster I/O

## Configuration Management

### Environment Variables
```bash
NODE_ENV=production
OXIGRAPH_ENDPOINT=http://oxigraph:7878
JENA_ENDPOINT=http://fuseki:3030
LOG_LEVEL=info
ENABLE_WASM=true
```

### Configuration Files
- Development: `.env.development`
- Testing: `.env.test`
- Production: `.env.production`

## Monitoring and Observability

### Metrics
- Query execution time
- Request rate
- Error rate
- Memory usage
- CPU utilization

### Logging
- Structured JSON logs
- Log levels: debug, info, warn, error
- Context-aware logging
- Log aggregation ready

### Tracing
- Request ID propagation
- Distributed tracing support
- Performance profiling

## Extension Points

### Adding New RDF Store Support

1. Implement `IRdfStoreAdapter` interface
2. Create adapter class in `src/adapters/`
3. Handle store-specific features
4. Add integration tests
5. Update documentation

### Adding New RDF Formats

1. Add format to `RdfFormat` enum
2. Implement parser in Rust
3. Update serializer
4. Add format conversion tests

### Custom Query Features

1. Extend `QueryOptions` type
2. Implement in adapters
3. Add query builder support
4. Document usage

## Future Enhancements

- [ ] GraphQL API layer
- [ ] Real-time streaming support (RDF Stream Processing)
- [ ] SHACL validation
- [ ] OWL reasoning support
- [ ] Distributed query federation
- [ ] Machine learning integration
- [ ] Visual query builder UI
- [ ] Multi-language bindings (Python, Java)

## References

- [SPARQL 1.1 Specification](https://www.w3.org/TR/sparql11-query/)
- [RDF 1.1 Specification](https://www.w3.org/TR/rdf11-concepts/)
- [Oxigraph Documentation](https://github.com/oxigraph/oxigraph)
- [Apache Jena Documentation](https://jena.apache.org/)
- [WebAssembly](https://webassembly.org/)

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for details on:
- Code style guidelines
- Testing requirements
- Pull request process
- Development workflow

## License

MIT License - See [LICENSE.md](LICENSE.md)

---

Maintained by: Janus RDF Team
Last Updated: 2024
Version: 0.1.0
WASM Adapter Status: Implemented