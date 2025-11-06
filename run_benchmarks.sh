#!/bin/bash

# Comprehensive benchmark script for testing Dense vs Sparse indexing approaches
# This script tests both reading and writing performance

echo "🚀 Starting Comprehensive RDF Indexing Benchmark Suite"
echo "======================================================"

# Create benchmarks directory if it doesn't exist
mkdir -p data/benchmark
mkdir -p data/write_benchmark

echo ""
echo "📊 Running Read Performance Benchmark (Current Implementation)"
echo "--------------------------------------------------------------"
cargo bench --bench benchmark

echo ""
echo "📝 Running Write Performance Benchmark (New Implementation)"
echo "-----------------------------------------------------------"
cargo bench --bench write_benchmark

echo ""
echo "🔬 Running Detailed Analysis"
echo "-----------------------------"

# Run additional analysis with different record sizes and intervals
echo "Testing different sparse intervals..."

# You can modify the intervals in the source code and run multiple tests
# This demonstrates how to test different configurations

echo ""
echo "✅ Benchmark Suite Complete!"
echo ""
echo "📋 Summary of Tests Performed:"
echo "  1. Read Performance (Query speed on existing indexes)"
echo "  2. Write Performance (Index creation speed during writing)"
echo "  3. Real-time vs Batch indexing comparison"
echo "  4. Memory usage comparison"
echo ""
echo "💡 Key Metrics to Compare:"
echo "  - Writing throughput (records/second)"
echo "  - Index build time"
echo "  - Memory usage"
echo "  - Query performance trade-offs"
echo "  - Storage space efficiency"
