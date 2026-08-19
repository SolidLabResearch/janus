# Paper Architecture

The diagrams show the two benchmark execution shapes. They are architectural
summaries; the executable benchmark binaries and their raw artifacts are the
source of performance evidence.

## Unified Janus execution

```mermaid
flowchart LR
    Q[Janus-QL query] --> P[Janus parser]
    P --> H[Historical storage query]
    P --> L[Live RSP-QL path]
    H --> M[Supported historical materialization]
    M --> L
    L --> R[Result stream]
```

## Decomposed comparison path

```mermaid
flowchart LR
    Q[Benchmark workload] --> H[Historical RDF events]
    H --> O[Oxigraph query]
    Q --> L[Janus live path]
    O --> J[In-process merge]
    L --> J
    J --> R[Comparison result]
```

## Nested historical materialization

```mermaid
flowchart TD
    Q[Nested historical SELECT] --> P[Validate and classify windows]
    P --> S[Historical SPARQL execution]
    S --> M[Materialize supported bindings]
    M --> L[Outer live execution]
```

See [Query Execution](./QUERY_EXECUTION.md) and
[PAPER_BENCHMARKING.md](./PAPER_BENCHMARKING.md) for the operational and
evidence boundaries.
