# Janus AI Coding Agent Instructions

## Project Overview

Janus is a Rust-based hybrid engine for unified Live and Historical RDF Stream Processing. It uses dictionary encoding to compress RDF quads from 40 bytes to 24 bytes (40% reduction) and achieves 2.6-3.14 Million quads/sec write throughput with sub-millisecond point query performance.

## Core Architecture

### Two-Layer Data Model

**User-Facing Layer (`RDFEvent`):**
- Full URI strings for subject, predicate, object, graph
- Used in all public APIs
- Example: `RDFEvent::new(timestamp, "http://example.org/alice", "http://example.org/knows", ...)`

**Internal Storage Layer (`Event`):**
- Fixed 24-byte struct with u32 dictionary IDs + u64 timestamp
- Used in storage and indexing
- Encoding/decoding happens transparently via Dictionary

**Critical Pattern:** Never expose internal `Event` structs in public APIs. Always convert between `RDFEvent` (user) and `Event` (storage) using `Dictionary::encode()` and `Dictionary::decode()`.

### Storage System (`src/storage/`)

**StreamingSegmentedStorage** is the core component:
- Uses `Arc<RwLock<BatchBuffer>>` for concurrent batch accumulation
- Background thread (`start_background_flushing()`) asynchronously flushes to disk
- Two-level sparse/dense indexing for range and point queries
- Dictionary persists separately via bincode serialization

**Threading Model:**
- Main thread: Accepts writes, adds to batch buffer
- Background thread: Monitors buffer, flushes when thresholds exceeded
- Use `Arc<RwLock<>>` for shared state, `Rc<RwLock<>>` for single-threaded Dictionary access
- Shutdown via `Arc<Mutex<bool>>` signal

### Dictionary Encoding (`src/storage/indexing/dictionary.rs`)

**Storage Convention:** Dictionary stores raw strings WITHOUT RDF syntax:
- URIs: `"https://example.org/resource"` (not `"<https://example.org/resource>"`)
- Literals: `"23.5"` (not `"23.5"^^xsd:double`)
- Datatypes tracked separately if needed

**Pattern:** Use `encode()` for URI→ID mapping, `decode()` for ID→URI retrieval. IDs are stable across restarts via `save_to_file()`/`load_from_file()`.

## Development Workflows

### Building and Testing

```bash
# Use Makefile for common tasks (colors, clean output)
make build          # Debug build
make release        # Optimized build (use for benchmarks)
make test           # All tests
make test-verbose   # With stdout capture disabled
make fmt            # Format all code
make clippy         # Lint checks
```

**CI Pipeline:** GitHub Actions runs `rustfmt`, `clippy`, and tests on Ubuntu/Windows/macOS with stable/beta Rust. See `.github/workflows/ci.yml`.

### Benchmarking

```bash
# Examples in examples/ directory (not benches/ - that's old)
cargo run --release --example realistic_rdf_benchmark
cargo run --release --example range_query_benchmark
cargo run --release --example point_query_benchmark
```

**Always use `--release`** for accurate performance measurements. Results documented in `BENCHMARK_RESULTS.md`.

### Testing Patterns

**Integration tests** (`tests/dictionary_encoding_test.rs`):
- Test full RDF workflows including dictionary persistence
- Use temporary directories with cleanup
- Follow existing patterns for IoT sensor simulation tests

**Unit tests:**
- Embedded in source files with `#[cfg(test)]`
- Test individual encoding/decoding, not full workflows

## Code Conventions

### Clippy Configuration

Extensive allow list in `src/lib.rs` for pedantic warnings. Key allowed patterns:
- `manual_div_ceil` - We implement manually for compatibility
- `cast_possible_truncation` - Intentional for u32 IDs from larger types
- `missing_docs_in_private_items` - Only public APIs need docs
- `needless_pass_by_value` - Prefer owned values for clarity

**Add new allows sparingly.** Document why if adding.

### Formatting

Uses `rustfmt.toml` with stable features only (no nightly):
- 100 char max width
- 4 spaces (no tabs)
- Edition 2021
- Run `make fmt` or `cargo fmt --all` before committing

### Naming Patterns

- Structs: `StreamingSegmentedStorage`, `Dictionary`, `Event`
- Methods: Snake_case verbs (`write()`, `encode()`, `start_background_flushing()`)
- Constants: `RECORD_SIZE`, `SPARSE_INTERVAL`
- Test functions: `test_<descriptive_name>` with underscores

## Common Tasks

### Adding a New RDF Event Field

1. Update `Event` struct in `src/core/mod.rs` (maintain 24-byte alignment if possible)
2. Update `RDFEvent` struct for user API
3. Update `encode_record()` and `decode_record()` in `src/core/encoding.rs`
4. Update `Dictionary::decode_graph()` formatting
5. Add tests in `tests/dictionary_encoding_test.rs`

### Modifying Storage Flush Behavior

Edit `StreamingConfig` in `src/storage/util.rs`:
- `max_batch_size_bytes` - Flush threshold
- `flush_interval_ms` - Time-based flush trigger
- `max_total_memory_mb` - Memory pressure limit

Then update `background_flush_loop()` logic in `src/storage/segmented_storage.rs`.

### Adding New Index Types

Follow patterns in `src/storage/indexing/`:
- `sparse.rs` - Sparse index (every Nth record)
- `dense.rs` - Dense index (every record)
- Implement builder function and reader struct
- Use binary search for timestamp lookups
- Add integration test with segment creation

## Performance Considerations

**Memory:** Use `VecDeque` for batch buffers (efficient push_back/pop_front). Avoid cloning large structures - use `Arc<RwLock<>>` for shared ownership.

**Concurrency:** Background flushing prevents write blocking. Use `.read()` for queries, `.write()` for mutations. Keep lock scopes minimal.

**Dictionary Size:** Grows linearly with unique URIs. Monitor `next_id` counter. For >4 billion unique strings, consider u64 IDs.

**Benchmarking:** Warm up caches before measurements. Use criterion or custom timing with multiple iterations. Document hardware specs in results.

## File Organization

- `src/core/` - Core types (`Event`, `RDFEvent`, encoding/decoding)
- `src/storage/` - Storage engine, indexing, memory tracking
- `src/indexing/` - Legacy indexing (prefer `src/storage/indexing/`)
- `src/parsing/` - JanusQL query parser (WIP)
- `tests/` - Integration tests (dictionary, storage workflows)
- `examples/` - Benchmark examples (use these, not old `benches/`)
- `data/` - Test data and benchmark outputs

## Debugging Tips

**Verbose test output:** `cargo test -- --nocapture --test-threads=1`

**Check dictionary state:** Use `Dictionary::decode_graph()` to print human-readable events during debugging.

**Memory tracking:** `MemoryTracker` in `src/storage/memory_tracker.rs` tracks allocations across platforms (Linux/macOS/Windows).

**CI failures:** Check formatting first (`cargo fmt --all -- --check`), then Clippy (`cargo clippy --all-targets --all-features`), then tests on specific OS.

## Documentation Standards

- Public APIs: Add `///` doc comments with examples
- Modules: Use `//!` at top of file
- Complex algorithms: Inline `//` comments explaining "why", not "what"
- See `src/storage/segmented_storage.rs` for well-documented examples

## External Dependencies

Minimal dependency philosophy:
- `serde` + `bincode` - Dictionary serialization
- `regex` - Query parsing
- Avoid heavy dependencies unless critical performance benefit

## Key Files Reference

- `src/storage/segmented_storage.rs` (725 lines) - Core storage implementation
- `src/storage/indexing/dictionary.rs` (172 lines) - Dictionary encoding
- `tests/dictionary_encoding_test.rs` (624 lines) - Comprehensive integration tests
- `BENCHMARK_RESULTS.md` - Performance metrics and analysis
- `ARCHITECTURE.md` - High-level design (note: mentions TypeScript, but actual impl is Rust)
- `Makefile` - Common development commands

# Copilot Instructions

## Code Style and Formatting

- Use consistent indentation (2 or 4 spaces, never tabs).
- Follow language-specific formatting standards (PEP 8 for Python, Google style guide for Java, etc.).
- Keep lines under 80 characters when possible, 120 maximum.
- Use meaningful variable and function names that clearly describe intent.
- Avoid abbreviations unless they are widely recognized domain terms.

## No Decorative Elements

- Do not include emojis, ASCII art, or decorative comments in code.
- Avoid unnecessary Unicode characters or special symbols in variable names.
- Keep comments technical and informative, not conversational or playful.
- Focus code on functionality and clarity over aesthetics.

## Documentation

- Write clear docstrings/doc comments for all public functions and classes.
- Include parameter descriptions, return types, and potential exceptions.
- For complex logic, add brief inline comments explaining the why, not the what.
- Keep README files focused and technical. Include setup, usage, and architecture.

## Testing

- Write unit tests for core logic and edge cases.
- Include integration tests for stream topologies.
- Test failure scenarios and recovery behavior explicitly.
- Aim for >80% code coverage on critical paths.

## Performance

- Profile code before optimizing; measure improvements.
- Avoid unnecessary object allocations in hot paths.
- Use lazy evaluation where applicable.
- Document performance assumptions and trade-offs.

## Version Control

- Write atomic commits with clear messages.
- Reference issue trackers when relevant.
- Keep commits focused on a single logical change.
- Avoid committing generated files or credentials.

## Dependencies

- Pin versions explicitly for reproducibility.
- Minimize external dependencies; justify each addition.
- Regularly audit for security vulnerabilities.
- Document why each dependency is needed.

## Code Review Standards

- All production code requires peer review.
- Reviewers should verify correctness, performance, and maintainability.
- Address feedback before merging.
- Use reviews as learning opportunities for the team.
