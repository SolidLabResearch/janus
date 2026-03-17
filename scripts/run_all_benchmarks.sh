#!/bin/bash
# Complete Janus SEMANTiCS 2026 Benchmark Suite
# Runs all three benchmarks (H1, H2, H4) and generates summary

set -e

echo "=========================================="
echo " Janus SEMANTiCS 2026 Benchmark Suite"
echo "=========================================="
echo ""
echo "Branch: $(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo 'unknown')"
echo "Commit: $(git rev-parse --short HEAD 2>/dev/null || echo 'unknown')"
echo ""

mkdir -p results data/citybench data/scale data/anomalies

echo "[1/7] Downloading CityBench dataset..."
if [ -f "scripts/download_citybench.sh" ]; then
    bash scripts/download_citybench.sh
else
    echo "  WARNING: download script not found, skipping"
fi

echo ""
echo "[2/7] Generating scaled datasets for H4..."
for size in 100000 250000 500000 1000000 2000000 5000000; do
    path="data/scale/${size}.nq"
    if [ ! -f "$path" ]; then
        echo "  Generating $((size / 1000000))M quads..."
        python3 scripts/generate_realistic_data.py \
            --size "$size" \
            --output "$path" 2>&1 | head -1
    fi
done

echo ""
echo "[3/7] Injecting anomalies for H2 (5 seeds)..."
for seed in 0 1 2 3 4; do
    echo "  Seed $seed..."
    python3 scripts/inject_anomalies.py \
        --input data/citybench/live_5min.nq \
        --spec data/anomalies/spec_20.json \
        --output "data/anomalies/live_seed_${seed}.nq" \
        --ground-truth "data/anomalies/gt_seed_${seed}.json" \
        --seed "$seed" 2>&1 | head -1
done

echo ""
echo "[4/7] Running H1 latency benchmark..."
echo "  (Measuring 4-stage pipeline across 3 sizes × 3 rates, 30 runs each)"
cargo run --release --example h1_latency_benchmark

echo ""
echo "[5/7] Running H2 correctness benchmark..."
echo "  (Detecting 20 anomalies × 5 seeds, measuring detection latency)"
cargo run --release --example h2_correctness_benchmark

echo ""
echo "[6/7] Running H4 scalability benchmark..."
echo "  (Testing 6 dataset sizes, 100K–5M quads, 30 runs each)"
cargo run --release --example h4_scalability_benchmark

echo ""
echo "[7/7] Generating summary report..."
python3 scripts/generate_summary.py

echo ""
echo "=========================================="
echo "✓ Complete. Results in results/"
echo "  - results/summary.md: Paper-ready summary"
echo "  - results/h1_*.csv: Latency breakdown & isolation"
echo "  - results/h2_*.csv: Anomaly detection correctness"
echo "  - results/h4_*.csv: Scalability analysis"
echo "=========================================="
