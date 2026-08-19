# Anomaly-Oriented Queries

Janus can evaluate threshold and comparison logic inside supported Janus-QL
query shapes. It is useful when the decision can be expressed as a filter or
calculation over live bindings and, where needed, a supported historical
materialization.

## Good fits

- thresholds over current readings;
- comparisons with a historical aggregate produced by a nested historical
  subquery;
- fixed-window reference calculations;
- per-entity `AVG`, `GROUP BY`, arithmetic, and `FILTER` expressions within
  the tested query surface.

## Boundaries

Janus is not a general anomaly-detection platform. It does not promise a
continuously maintained, general historical/live relation, automatic model
training, or unsupported SPARQL/RSP-QL features. Query behavior is limited by
the parser and execution paths documented in [Janus-QL](./JANUSQL.md).

For compatibility baseline behavior, see [BASELINES.md](./BASELINES.md).
