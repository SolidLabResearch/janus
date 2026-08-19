# Paper Submission Package

## Include

- The source revision or immutable release identifier.
- The canonical current docs: README, Janus-QL, HTTP API, execution, and
  benchmark guide.
- A result package for each stated claim: raw rows, summaries, commands,
  environment metadata, and generated figures/tables.
- Query fixtures and correctness/equivalence evidence needed to interpret the
  result.

## Exclude by default

- Build directories such as `target/`.
- Generated event data and scratch benchmark directories.
- Local storage directories, broker data, logs, and editor artifacts.
- Derived figures or summaries without their provenance package.

## Check before submitting

- Ensure every reported number names the workload, machine, sample size, and
  statistic.
- Check that the report distinguishes measured `0` from not-applicable `N/A`.
- Verify that generated artifacts match the claimed Git revision.
- Do not substitute a historical result snapshot for a current run.
