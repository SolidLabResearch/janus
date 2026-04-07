#!/bin/bash
# CI/CD Pre-Push Checks
# Run this script before pushing to ensure CI/CD will pass

set -e  # Exit on any error

# Ensure we are in the project root
cd "$(dirname "$0")/.."

echo "Running CI/CD checks locally..."
echo ""

# Check 1: Format check
echo "Checking code formatting (rustfmt)..."
cargo fmt --all -- --check
echo "Formatting check passed!"
echo ""

# Check 2: Clippy lints
echo "Running Clippy lints..."
cargo clippy --all-targets --all-features -- -D warnings
echo "Clippy check passed!"
echo ""

# Check 3: Run tests
echo "Running test suite..."
cargo test --all-targets
echo "All tests passed!"
echo ""

# Check 4: Build check
echo "Building project..."
cargo build --all-targets
echo "Build successful!"
echo ""

echo "All CI/CD checks passed! Safe to push."
