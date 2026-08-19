# Janus-QL Window Types

Janus uses named windows to make the data source and time bounds explicit.

## Historical fixed

```sparql
FROM NAMED WINDOW ex:history ON LOG ex:log [START 1700000000000 END 1700086400000]
```

Evaluates once over a persisted event-log interval. `END` must be later than
`START`.

## Historical sliding

```sparql
FROM NAMED WINDOW ex:previousHour ON LOG ex:log [OFFSET 86400000 RANGE 3600000 STEP 30000]
```

At time `T`, Janus evaluates `[T - OFFSET - RANGE, T - OFFSET]`. The range
cannot exceed the offset.

## Live sliding

```sparql
FROM NAMED WINDOW ex:live ON STREAM ex:stream [RANGE 60000 STEP 30000]
```

Evaluates arriving stream data in a moving window. The range and step must be
positive.

## Hybrid

Declare a log and a stream window in the same query to combine historical and
live work. Use a `WINDOW <name> { … }` block only after declaring `<name>`.
See [Janus-QL](./JANUSQL.md) for complete examples.
