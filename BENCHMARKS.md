# Janus Benchmark Reproducibility Guide

This document explains how to reproduce the SEMANTiCS 2026 benchmark results for Janus,
a hybrid engine for unified live and historical RDF stream processing.

## Quick Start: Run All Benchmarks

If you have Docker installed:

```bash
docker build -f Dockerfile.bench -t janus-bench .
docker run -v $(pwd)/results:/janus/results janus-bench
```

Otherwise, run directly:

```bash
bash scripts/run_all_benchmarks.sh
```

Both approaches generate complete results in `results/` with a summary in `results/summary.md`.

---

## Three Experimental Hypotheses

| # | Hypothesis | Experiment | Result Files |
|---|-----------|-----------|--------------|
| **H1** | Unified query architecture provides efficient end-to-end latency with path isolation | 4-stage pipeline breakdown; background load test | `h1_summary.csv`; `h1_isolation.csv` |
| **H2** | Real-time anomaly detection via unified historical+live comparison | 20 anomalies × 5 seeds, measure detection latency | `h2_detection.csv`; `h2_summary.csv` |
| **H4** | Two-level sparse index enables sub-linear scaling to millions of quads | Historical retrieval time vs. dataset size (100K–5M) | `h4_scalability.csv`; `h4_summary.csv` |

---

## What Gets Measured

### H1 — End-to-End Latency Breakdown

**Configuration:** 3 dataset sizes (50K, 100K, 500K quads) × 3 event rates (50, 100, 500 /sec) × 30 runs

**Four Pipeline Stages:**
1. **Storage write latency** — RDF event → batch buffer
2. **Historical retrieval latency** — SPARQL query execution on stored data
3. **Live window close latency** — Event triggers window closure → result
4. **Result combination latency** — Comparator processing

**Path Isolation Sub-test:**
- Background historical queries at 0, 1, 5, 10 queries/sec
- Live window latency should **remain flat** (no interference)

**Output:**
- `h1_latency.csv` — All 2,700 raw measurements
- `h1_summary.csv` — Mean/std_dev per configuration
- `h1_isolation.csv` — Live latency under background load

---

### H2 — Anomaly Detection Correctness

**Setup:**
1. Load 25-minute historical baseline
2. Register Janus-QL query with historical + live windows
3. Replay 5-minute live stream with 20 injected anomalies
4. Detect when live average deviates >10% from historical
5. Record detection latency per anomaly
6. **Repeat 5 times** with different random seeds

**Anomaly Types Tested:**
- `stuck_sensor` — Constant value for duration
- `spike` — Single-timestamp multiplier spike
- `sustained_drop` — Percentage reduction over duration
- `gradual_drift` — Linear value increment per step

**Output:**
- `h2_detection.csv` — Per-anomaly detection metrics (seed × anomaly)
- `h2_summary.csv` — Aggregate: detection rate, mean latency, false positives

---

### H4 — Scalability Analysis

**Configuration:** 6 dataset sizes (100K, 250K, 500K, 1M, 2M, 5M quads) × 30 runs per size

**Three Measurements Per Run:**
1. **Historical retrieval latency** — Time to query 10% of dataset via SPARQL
2. **Bootstrap latency** — Time from `start_query()` to first historical result
3. **Live window latency** — Closure-to-result time (should stay flat)

**Index Effectiveness:**
- Plots historical retrieval time vs. dataset size
- Passes if latency ratio < size ratio (sub-linear growth)
- Confirms two-level sparse index is working

**Output:**
- `h4_scalability.csv` — Raw per-run measurements
- `h4_summary.csv` — Mean/std_dev with sub-linearity check (PASS/WARN/baseline)

---

## Benchmark Binaries

Each hypothesis has a dedicated example binary:

```bash
# H1: Latency breakdown
cargo run --release --example h1_latency_benchmark
# Output: results/h1_latency.csv, h1_summary.csv, h1_isolation.csv

# H2: Anomaly detection
cargo run --release --example h2_correctness_benchmark
# Output: results/h2_detection.csv, h2_summary.csv

# H4: Scalability
cargo run --release --example h4_scalability_benchmark
# Output: results/h4_scalability.csv, h4_summary.csv
```

**Important:** All benchmarks **require `--release` mode.** Debug builds produce meaningless latency numbers.

---

## Dataset

### Automatic Download

The script `scripts/download_citybench.sh` automatically:
1. Downloads CityBench AarhusTrafficData from http://www.ict-citypulse.eu/citybench/
2. Converts to N-Quads format with timestamps
3. Creates three slices:
   - `data/citybench/historical_25min.nq` — H2's historical baseline
   - `data/citybench/live_5min.nq` — H2's live stream (anomaly injection target)
   - `data/citybench/full.nq` — Complete dataset

### Network Unavailable?

If the download fails, `scripts/generate_realistic_data.py` generates synthetic data:

```bash
python3 scripts/generate_realistic_data.py --size 100000 --output data/synthetic.nq
```

---

## Anomaly Injection

H2 requires 20 anomalies injected across 5 seeds. This is **automatic** in the benchmark script:

```bash
python3 scripts/inject_anomalies.py \
    --input data/citybench/live_5min.nq \
    --spec data/anomalies/spec_20.json \
    --output data/anomalies/live_seed_0.nq \
    --ground-truth data/anomalies/gt_seed_0.json \
    --seed 0
```

Specifications:
- `data/anomalies/spec.json` — 5 anomalies (one of each type)
- `data/anomalies/spec_20.json` — 20 anomalies (5 per type, used for paper)

---

## Results Summary

After running all benchmarks, read `results/summary.md`:

```
# Janus Benchmark Results — SEMANTiCS 2026

[Hardware spec]

## H1 — End-to-End Latency
[Table with mean/std per configuration]

## H2 — Anomaly Detection
[Detection rate, latency, false positives]

## H4 — Scalability
[Latency vs. size with sub-linearity check]

## Hypothesis Mapping
[CSV files → Paper sections]
```

---

## Hardware Spec

Results are recorded with hardware information at:
- `results/hardware.txt` — CPU model, cores, RAM

**Absolute numbers vary by hardware; relative trends should be reproducible.**

Example (MacBook Pro M1):
```
CPU: Apple M1
Memory: 8 GB
OS: macOS 13.0
```

---

## Customization

### Change Dataset Size

Edit constants in benchmark files:

**h1_latency_benchmark.rs:**
```rust
const DATASET_SIZES: &[usize] = &[50_000, 100_000, 500_000];
const EVENT_RATES_PER_SEC: &[u64] = &[50, 100, 500];
```

**h4_scalability_benchmark.rs:**
```rust
const SIZES: &[usize] = &[100_000, 250_000, 500_000, 1_000_000, 2_000_000, 5_000_000];
```

### Change Anomaly Count

Modify `data/anomalies/spec_*.json`:
- `spec.json` — 5 anomalies (quick test)
- `spec_20.json` — 20 anomalies (paper evaluation, robust)

### Run Individual Benchmarks

```bash
# Just H1
cargo run --release --example h1_latency_benchmark

# Just H2
cargo run --release --example h2_correctness_benchmark

# Just H4
cargo run --release --example h4_scalability_benchmark

# Skip dataset download
rm data/citybench/*.nq  # prevents redownload
```

---

## Troubleshooting

### "Benchmarks MUST be run with --release"

You tried to run a benchmark in debug mode. Use:
```bash
cargo run --release --example h1_latency_benchmark
```

### "Script not found: download_citybench.sh"

Ensure you're in the Janus repo root and the script has execute permissions:
```bash
chmod +x scripts/download_citybench.sh
bash scripts/download_citybench.sh
```

### "Timeout waiting for historical result"

Large datasets may take longer. Increase the timeout in the benchmark file (search for `t_bootstrap.elapsed`).

### "Cannot find generated dataset"

Datasets are generated on-demand in `data/scale/` or `data/citybench/`.
Ensure write permissions and sufficient disk space (~20GB for all sizes).

---

## Citation

If you use these benchmarks, please cite:

```bibtex
@inproceedings{janus2026semantics,
  title={Janus: Unified Live and Historical RDF Stream Processing},
  booktitle={Proceedings of SEMANTiCS 2026, Resource Track},
  author={Bisen, Kush and others},
  year={2026}
}
```

---

## Further Reading

- **Janus Repository:** https://github.com/SolidLabResearch/janus
- **CityBench:**  http://www.ict-citypulse.eu/citybench/
- **SEMANTiCS:** https://2026.semantics.cc/

