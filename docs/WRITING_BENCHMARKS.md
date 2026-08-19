# Writing Janus Benchmarks

Use Cargo Criterion benchmarks for stable microbenchmarks and a named binary
under `src/bin/` for multi-step experiments that need files, configuration, or
cross-system orchestration.

## Requirements

- State the measured path and metric precisely.
- Generate deterministic data or preserve the exact input fixture.
- Keep setup, warm-up, and measured work distinct.
- Emit raw per-iteration data before summary statistics.
- Record command, Git SHA, Rust version, machine details, and run date.
- Validate result correctness separately from performance.
- Use `N/A` for steps absent from an approach; do not serialize them as zero.

Run `cargo bench --no-run` before an expensive campaign. Add the benchmark to
[BENCHMARKING.md](./BENCHMARKING.md) when it becomes a maintained target.
