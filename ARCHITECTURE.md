# Janus RDF Template - Architecture Documentation

## Overview

Janus is a TypeScript-based framework designed for efficient RDF (Resource
Description Framework) data store integration and stream processing. It provides
a unified interface for interacting with multiple RDF triple stores through HTTP
APIs.

## Design Principles

1. Adapter Pattern: Provide consistent interfaces for different RDF store
   implementations
2. Type Safety: Full TypeScript type definitions with strict typing enabled
3. Performance: Efficient HTTP-based communication with remote RDF stores
4. Testability: Comprehensive test coverage with both unit and integration tests
5. Extensibility: Easy to add new RDF store adapters and formats
6. Stream Processing: Support for both historical and live RDF stream processing

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
│  │  │ Oxigraph   │  │    Jena    │  │ In-Memory  │ │           │
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
└─────────────┬───────────────────────────────────────────────────┘
              │ HTTP
┌─────────────▼──────────────────────────────────────────────────┐
│                    External RDF Stores                         │
│  ┌──────────────────┐  ┌──────────────────┐                   │
│  │    Oxigraph      │  │    Jena Fuseki   │                   │
│  │  Server (HTTP)   │  │    Server (HTTP) │                   │
│  └──────────────────┘  └──────────────────┘                   │
└────────────────────────────────────────────────────────────────┘
```

## Core Components

### 1. TypeScript Layer

#### Core Types & Interfaces (`src/core/types.ts`)

**Enums:**

- RdfFormat: Supported RDF serialization formats (Turtle, N-Triples, RDF/XML,
  JSON-LD, N-Quads, TriG)
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

#### InMemoryAdapter (`src/adapters/InMemoryAdapter.ts`)

- Simple in-memory RDF triple storage
- JavaScript-based implementation using N3.js
- No HTTP overhead
- Features:
  - Fast local operations
  - Suitable for testing and development
  - Limited scalability

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
    └─► [InMemory Adapter] ──► N3.js Store ──► Query Result
                                (Local/In-memory)
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
    └─► [InMemory Adapter]
            │
            ├─► Parse with N3.js
            ├─► Parse RDF triples
            ├─► Insert into N3 store
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

### JavaScript/Node.js

- Runtime: Node.js 18+
- RDF Libraries: N3.js, rdf-parse
- Stream Processing: rsp-js
- Messaging: KafkaJS, MQTT

## Key Design Patterns

### 1. Adapter Pattern

All RDF store integrations implement the `IRdfStoreAdapter` interface, allowing
seamless switching between different backends.

```typescript
interface IRdfStoreAdapter {
  loadData(
    data: string,
    format: RdfFormat,
    graphName?: string
  ): Promise<number>;
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
    case 'oxigraph':
      return new OxigraphAdapter(config);
    case 'jena':
      return new JenaAdapter(config);
    default:
      throw new Error('Unknown store type');
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

### 1. HTTP Performance

- Pros: Distributed architecture, scalability, persistent storage
- Cons: Network latency, serialization overhead
- Use Case: Large datasets, shared stores, production deployments

### 2. In-Memory Performance

- Pros: Fast local operations, no network overhead
- Cons: Limited by available memory, not persistent
- Use Case: Testing, development, small datasets

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
- Testing framework: Jest

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

Maintained by: Janus RDF Team Last Updated: 2024 Version: 0.1.0 WASM Adapter
Status: Implemented
