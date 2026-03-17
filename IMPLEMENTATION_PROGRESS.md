# SEMANTiCS 2026 Benchmarking Implementation Progress

## Completed ✓

### Phase 0 — Bug Fixes (3/3)
- ✅ `generate_segment_id()`: Fixed segment ID collision with AtomicU64 counter
- ✅ `load_index_directory_from_file()`: Fixed hardcoded entries_per_block, now accepts config parameter
- ✅ Dictionary persistence: Now saves to disk on shutdown and loads on startup

### Phase 1 — Core Api Implementation
- ✅ `JanusApi::push_event()`: Implemented with event channel integration into live worker thread
  - Allows programmatic event injection into running live queries
  - Thread-safe via mpsc channel
  - Returns proper error handling for non-running queries or missing live windows

### Phase 2 — Dataset Preparation Scripts
- ✅ `scripts/download_citybench.sh`: Downloads AarhusTrafficData with fallback to synthetic data
- ✅ `scripts/convert_to_nquads.py`: Converts TTL/N3 to N-Quads with timestamp extraction
- ✅ `scripts/inject_anomalies.py`: Injects 5 anomaly types (stuck_sensor, spike, sustained_drop, gradual_drift)
- ✅ Anomaly specs: `spec.json` (5 anomalies) and `spec_20.json` (20 anomalies for robust testing)

### Shared Infrastructure
- ✅ `src/benchmarking.rs`: Utility module with:
  - `analyse_runs()`: Discards warmup (first 3) and outliers (last 2), returns mean/std_dev
  - `get_hardware_info()`: Cross-platform CPU and memory detection

## Files Created
- Core logic: `src/api/janus_api.rs` (push_event), `src/storage/segmented_storage.rs` (3 fixes)
- Scripts: `scripts/{download_citybench.sh, convert_to_nquads.py, inject_anomalies.py}`
- Specs: `data/anomalies/{spec.json, spec_20.json}`
- Infrastructure: `src/benchmarking.rs`

## Remaining Work

### Phase 3 — H1 Latency Benchmark
File: `examples/h1_latency_benchmark.rs`
- Measures 4-stage pipeline: storage write, historical retrieval, live window close, result combination
- Configurations: 3 dataset sizes × 3 event rates × 30 runs = 270 benchmark runs
- Output: `results/h1_latency.csv`, `results/h1_summary.csv`, `results/h1_isolation.csv`

### Phase 4 — H2 Correctness Benchmark
File: `examples/h2_correctness_benchmark.rs`
- Tests anomaly detection accuracy and latency
- Runs query on historical baseline, then replays live stream with injected anomalies
- Outputs: `results/h2_detection.csv`, `results/h2_summary.csv`
- Runs 5 times with different random seeds for statistical robustness

### Phase 5 — H4 Scalability Benchmark
File: `examples/h4_scalability_benchmark.rs`
- Measures how historical retrieval scales with dataset size
- 6 dataset sizes (100K to 5M quads) × 30 runs
- Confirms sub-linear growth (two-level index effectiveness)
- Outputs: `results/h4_scalability.csv`, `results/h4_summary.csv`

### Phase 6 — Reproducibility Package
- `Dockerfile.bench`: Multi-stage Docker build with Rust, Python, dependencies
- `scripts/run_all_benchmarks.sh`: End-to-end orchestration script
- `scripts/generate_summary.py`: Aggregates results into `results/summary.md`
- `BENCHMARKS.md`: Root-level documentation mapping hypotheses → result files

## Key Integration Points

1. **Storage**: Uses `StreamingSegmentedStorage` with fixed bugs
2. **API**: Uses `JanusApi::start_query()` and `push_event()`
3. **Historical**: Uses `HistoricalExecutor` with fixed configuration handling
4. **Live**: Uses `LiveStreamProcessing` with programmatic event injection
5. **Results**: All benchmark results route through unified `QueryResult` channel

## Execution Flow

```
Phase 3 (H1):
├─ Load dataset (N-Quads)
├─ Initialize storage and API
├─ For each config (size, rate):
│  └─ 30 runs of: write → query start → result collection
└─ Analyze results, output CSVs

Phase 4 (H2):
├─ Load historical portion (25 min)
├─ Register query with historical+live windows
├─ Start query (measure bootstrap time)
├─ Replay live portion with anomalies (5 min)
├─ For each anomaly: measure detection latency
└─ Repeat 5 times with different seeds

Phase 5 (H4):
├─ For each dataset size:
│  └─ 30 runs of: load dataset → start query → measure times
└─ Confirm hist latency < live latency (path isolation)
```

## Recommended Next Steps

1. **Implement H1 Benchmark** (most straightforward, sets pattern for others)
   - Handle: CSV writing, timing with `std::time::Instant`, run analysis
   - Use `tempfile` crate for isolated benchmark runs

2. **Implement H2 Benchmark** (anomaly detection logic)
   - Handle: Ground truth parsing, detection threshold comparison, seed management

3. **Implement H4 Benchmark** (scalability verification)
   - Handle: Dataset generation via `generate_realistic_data.py`, size verification

4. **Create Docker & Scripts** (packaging for reproducibility)
   - Dockerfile should match "release" profile for fair results
   - Shell script orchestrates all 3 benchmarks in sequence

5. **Generate Final Summary** (Python aggregation)
   - Read CSVs, detect trends (should be sub-linear for H4)
   - Verify anomaly detection rate > 80% for H2
   - Check path isolation (live latency constant across H1 background load)

## Testing

Before final submission:
```bash
# Verify bug fixes work
cargo test

# Run in release mode (critical!)
cargo run --release --example h1_latency_benchmark

# Verify Docker build
docker build -f Dockerfile.bench .
docker-compose run janus-bench
```

## Notes

- All timing must use `std::time::Instant` (not `SystemTime`)
- All benchmarks must use `--release` flag
- Store hardware spec at start of each run
- Use 10ms sleep in polling loops to avoid busy-waiting
- Report warmup discarded (first 3) and outliers discarded (last 2)
