# Baseline Compatibility

The current implementation retains baseline-oriented paths for benchmark and
API compatibility. They are not part of the public Janus-QL contract described
in [JANUSQL.md](./JANUSQL.md), and applications should not depend on them for
query portability.

The implementation supports historical bootstrap data being prepared before
or alongside live execution. This can cause a registered query to report
`WarmingBaseline` while the bootstrap work is in progress.

For current application development, prefer an explicit fixed or sliding
historical window and a supported nested historical subquery. That path is
covered by [Nested Historical Subqueries](./NESTED_HISTORICAL_SUBQUERIES.md)
and the parser's public-spec tests.

Benchmark code that uses legacy `USING BASELINE` or `DEFINE BASELINE` syntax
must be treated as implementation compatibility code: preserve its existing
shape, keep its scope local, and validate it against the current runtime before
using it in a result claim.
