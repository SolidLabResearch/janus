# Changelog

All notable changes to the Janus project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **Historical Sliding Window Operator** (`src/stream/operators/historical_sliding_window.rs`)
  - Implements `Iterator` pattern for sliding windows over historical data
  - Supports configurable width, slide, and offset parameters
  - Uses `SystemTime::now()` internally to determine window boundaries
  - Clamps window end to prevent querying beyond current time
  - Mandatory offset parameter for "go back" semantics

- **Historical Fixed Window Operator** (`src/stream/operators/historical_fixed_window.rs`)
  - Single-query operator for fixed time ranges [start, end]
  - Implements `Iterator` pattern (yields once, then returns `None`)
  - Enforces mandatory start and end timestamps

- **Integration Tests with Real RDF IRIs**
  - `tests/historical_sliding_window_test.rs`: Sensor data and FOAF examples
  - `tests/historical_fixed_window_test.rs`: IoT devices and semantic web publications
  - Uses standard vocabularies: RDF, FOAF, Dublin Core, BIBO

### Changed
- Consolidated window definitions to use `WindowDefinition` from `janusql_parser.rs` consistently
- Removed duplicate `HistoricalWindow` struct from `src/stream/operators/hs2r.rs`
- Moved `historical_sliding_window.rs` into `src/stream/operators/` directory for better organization

### Fixed
- Window end clamping to prevent querying future data in sliding windows
- Type consistency across window operators

## [0.1.0] - Initial Release

### Added
- Core RDF event processing
- Segmented storage with two-level indexing
- JanusQL parser with support for live, historical sliding, and historical fixed windows
- Dictionary encoding for efficient RDF storage
- Basic streaming infrastructure

---

**Note**: This changelog tracks changes starting from the implementation of historical window operators. Earlier changes may not be fully documented.
