# JANUS-QL Parser

A TypeScript/JavaScript parser that extends RSP-QL (RDF Stream Processing Query Language) with support for historical data windows, enabling unified queries across both live streaming data and historical archives.

## Overview

The JANUS-QL parser takes a single query that combines live and historical data windows and separates it into:

1. **RSP-QL Query** - For processing live streaming data in real-time
2. **SPARQL Queries** - For querying historical data from persistent storage

This allows you to write one query that spans both temporal dimensions of your data.

## Key Features

- ✅ **Live Windows** - Standard RSP-QL sliding/tumbling windows for real-time streams
- ✅ **Historical Sliding Windows** - Time-offset windows for querying recent historical data
- ✅ **Historical Fixed Windows** - Absolute timestamp ranges for historical queries
- ✅ **Prefix Support** - Full PREFIX expansion and compression
- ✅ **Type Safety** - Written in TypeScript with complete type definitions
- ✅ **Clean Separation** - Automatically splits queries into live and historical components

## Installation

```typescript
import { JanusQLParser } from './rsp/JanusQLParser';
```

## Quick Start

```typescript
import { JanusQLParser } from './rsp/JanusQLParser';

const query = `
PREFIX : <https://rsp.js/schema#>
REGISTER RStream :output AS
SELECT *
FROM NAMED WINDOW :live ON STREAM :sensors [RANGE 1000 STEP 100]
FROM NAMED WINDOW :historical ON STREAM :sensors [OFFSET 3600000 RANGE 1000 STEP 100]
WHERE {
  WINDOW :live { ?s ?p ?o }
}
`;

const parser = new JanusQLParser();
const result = parser.parse(query);

console.log('Live Windows:', result.liveWindows);
console.log('Historical Windows:', result.historicalWindows);
console.log('RSP-QL Query:', result.rspqlQuery);
console.log('SPARQL Queries:', result.sparqlQueries);
```

## Window Types

### 1. Live Sliding Window

For real-time stream processing with overlapping windows.

```sparql
FROM NAMED WINDOW :myWindow ON STREAM :myStream [RANGE 1000 STEP 100]
```

- **RANGE** - Window width in milliseconds
- **STEP** - Slide interval in milliseconds
- **Use Case** - Continuous monitoring with frequent updates

### 2. Live Tumbling Window

Non-overlapping windows where `RANGE = STEP`.

```sparql
FROM NAMED WINDOW :myWindow ON STREAM :myStream [RANGE 1000 STEP 1000]
```

- **Use Case** - Periodic aggregations without overlap

### 3. Historical Sliding Window

Query historical data with a time offset from the present.

```sparql
FROM NAMED WINDOW :pastWindow ON STREAM :myStream [OFFSET 3600000 RANGE 1000 STEP 100]
```

- **OFFSET** - Time offset from now (milliseconds ago)
- **RANGE** - Window width in milliseconds  
- **STEP** - Slide interval for multiple windows
- **Use Case** - Compare current behavior with past behavior

### 4. Historical Fixed Window

Query a specific absolute time range.

```sparql
FROM NAMED WINDOW :fixedWindow ON STREAM :myStream [START 1700000000 END 1700086400]
```

- **START** - Unix timestamp (seconds)
- **END** - Unix timestamp (seconds)
- **Use Case** - Analyze specific historical events or periods

### 5. Historical Tumbling Window

Non-overlapping historical windows where `RANGE = STEP`.

```sparql
FROM NAMED WINDOW :pastTumbling ON STREAM :myStream [OFFSET 3600000 RANGE 1000 STEP 1000]
```

- **Use Case** - Historical aggregations without overlap

## Complete Example

```sparql
PREFIX : <https://rsp.js/schema#>
PREFIX sensor: <https://example.org/sensors/>

REGISTER RStream :accelAnalysis AS
SELECT ?s ?acceleration ?avgHistorical
WHERE {
  # Live data - last 1 second, updated every 10ms
  WINDOW :liveWindow {
    ?s a sensor:Accelerometer .
    ?s :timestamp ?timestamp .
    ?s :acceleration ?acceleration .
  }
  
  # Historical data - 1 hour ago, 1 second window
  WINDOW :historicalWindow {
    ?hs a sensor:Accelerometer .
    ?hs :timestamp ?hTimestamp .
    ?hs :acceleration ?hAcceleration .
  }
  
  # Compare current with historical average
  FILTER(?acceleration > ?avgHistorical * 1.5)
}

FROM NAMED WINDOW :liveWindow ON STREAM :accelStream [RANGE 1000 STEP 10]
FROM NAMED WINDOW :historicalWindow ON STREAM :accelStream [OFFSET 3600000 RANGE 1000 STEP 1000]
```

### Generated RSP-QL (Live)

```sparql
PREFIX : <https://rsp.js/schema#>
PREFIX sensor: <https://example.org/sensors/>

REGISTER RStream :accelAnalysis AS
SELECT ?s ?acceleration ?avgHistorical

FROM NAMED WINDOW :liveWindow ON STREAM :accelStream [RANGE 1000 STEP 10]
WHERE {
  WINDOW :liveWindow {
    ?s a sensor:Accelerometer .
    ?s :timestamp ?timestamp .
    ?s :acceleration ?acceleration .
  }
}
```

### Generated SPARQL (Historical)

```sparql
PREFIX : <https://rsp.js/schema#>
PREFIX sensor: <https://example.org/sensors/>

SELECT ?s ?acceleration ?avgHistorical

FROM NAMED :historicalWindow

WHERE {
  GRAPH :historicalWindow {
    ?hs a sensor:Accelerometer .
    ?hs :timestamp ?hTimestamp .
    ?hs :acceleration ?hAcceleration .
  }
  # Historical sliding window: offset=3600000, range=1000, step=1000
}
```

## API Reference

### `JanusQLParser`

Main parser class for JANUS-QL queries.

#### Methods

##### `parse(query: string): ParsedJanusQuery`

Parses a JANUS-QL query string and returns structured results.

**Parameters:**
- `query` - The JANUS-QL query string

**Returns:** `ParsedJanusQuery` object containing:

```typescript
interface ParsedJanusQuery {
  // R2S operator (RStream, IStream, DStream)
  r2s: R2SOperator | null;
  
  // Windows for live streaming
  liveWindows: WindowDefinition[];
  
  // Windows for historical data
  historicalWindows: WindowDefinition[];
  
  // Generated RSP-QL for live processing
  rspqlQuery: string;
  
  // Generated SPARQL queries (one per historical window)
  sparqlQueries: string[];
  
  // Parsed PREFIX mappings
  prefixes: Map<string, string>;
  
  // Extracted WHERE clause
  whereClause: string;
  
  // Extracted SELECT clause
  selectClause: string;
}
```

### Data Types

#### `WindowDefinition`

Describes a single window (live or historical).

```typescript
interface WindowDefinition {
  window_name: string;    // Fully expanded IRI
  stream_name: string;    // Fully expanded IRI
  width: number;          // RANGE value (milliseconds)
  slide: number;          // STEP value (milliseconds)
  offset?: number;        // OFFSET value (for historical-sliding)
  start?: number;         // START timestamp (for historical-fixed)
  end?: number;           // END timestamp (for historical-fixed)
  type: 'live' | 'historical-sliding' | 'historical-fixed';
}
```

#### `R2SOperator`

Relation-to-Stream operator information.

```typescript
interface R2SOperator {
  operator: string;  // 'RStream', 'IStream', or 'DStream'
  name: string;      // Output stream name (expanded IRI)
}
```

## Testing

Run the included test script:

```bash
node test-janus-ql.js
```

Or use the TypeScript example:

```bash
npx ts-node src/rsp/examples/janus-ql-example.ts
```

## Use Cases

### 1. Anomaly Detection

Compare live sensor readings against historical baselines.

```sparql
FROM NAMED WINDOW :current ON STREAM :sensors [RANGE 5000 STEP 1000]
FROM NAMED WINDOW :baseline ON STREAM :sensors [START 1700000000 END 1700086400]
```

### 2. Trend Analysis

Monitor current trends and compare with different historical periods.

```sparql
FROM NAMED WINDOW :now ON STREAM :metrics [RANGE 60000 STEP 5000]
FROM NAMED WINDOW :lastHour ON STREAM :metrics [OFFSET 3600000 RANGE 60000 STEP 5000]
FROM NAMED WINDOW :lastDay ON STREAM :metrics [OFFSET 86400000 RANGE 60000 STEP 5000]
```

### 3. Real-time Dashboard with Context

Show live data alongside historical context for informed decision-making.

```sparql
FROM NAMED WINDOW :live ON STREAM :events [RANGE 10000 STEP 1000]
FROM NAMED WINDOW :recent ON STREAM :events [OFFSET 300000 RANGE 10000 STEP 5000]
```

### 4. Capacity Planning

Monitor current load against historical peak periods.

```sparql
FROM NAMED WINDOW :currentLoad ON STREAM :system [RANGE 30000 STEP 5000]
FROM NAMED WINDOW :peakPeriod ON STREAM :system [START 1700000000 END 1700003600]
```

## Implementation Details

### Parsing Strategy

1. **Line-by-line processing** - Handles multi-line queries gracefully
2. **PREFIX extraction** - Collects and expands all PREFIX declarations
3. **Window classification** - Uses regex patterns to identify window types
4. **Query separation** - Splits into live (RSP-QL) and historical (SPARQL) queries
5. **IRI management** - Unwraps and rewraps IRIs as needed

### Window Type Detection

The parser uses the following logic to classify windows:

```
IF line contains [OFFSET ... RANGE ... STEP ...] 
  → historical-sliding

ELSE IF line contains [START ... END ...]
  → historical-fixed

ELSE IF line contains [RANGE ... STEP ...]
  → live
```

### WHERE Clause Transformation

For historical SPARQL queries:
- `WINDOW` keywords are replaced with `GRAPH`
- Timestamp filters are added for fixed windows
- Comments are added for sliding windows

## Limitations

### Current Limitations

1. **WHERE Clause Sharing** - All historical SPARQL queries share the same WHERE clause structure
2. **No Dynamic Filtering** - Historical sliding windows don't auto-inject computed timestamp filters
3. **Single R2S Operator** - Only one REGISTER statement per query
4. **Comment-based Metadata** - Window parameters for sliding historical windows are preserved as comments

### Planned Enhancements

- [ ] Per-window WHERE clause customization
- [ ] Automatic timestamp variable injection and filtering
- [ ] Support for CONSTRUCT queries
- [ ] Multiple R2S operators
- [ ] Window join optimization hints
- [ ] Temporal reasoning operators
- [ ] Cross-window aggregation functions

## Architecture

```
┌─────────────────┐
│  JANUS-QL Query │
└────────┬────────┘
         │
         ▼
   ┌──────────┐
   │  Parser  │
   └─────┬────┘
         │
    ┌────┴────┐
    │         │
    ▼         ▼
┌────────┐ ┌──────────┐
│ Live   │ │Historical│
│Windows │ │ Windows  │
└───┬────┘ └────┬─────┘
    │           │
    ▼           ▼
┌─────────┐ ┌──────────┐
│ RSP-QL  │ │ SPARQL   │
│ Query   │ │ Queries  │
└─────────┘ └──────────┘
```

## Related Technologies

- **RSP-QL** - [RDF Stream Processing Query Language](https://streamreasoning.org/RSP-QL/)
- **SPARQL** - [SPARQL 1.1 Query Language](https://www.w3.org/TR/sparql11-query/)
- **rsp-js** - RSP-QL JavaScript implementation

## Contributing

When extending the parser, consider:

1. Maintaining backward compatibility with standard RSP-QL
2. Adding comprehensive type definitions
3. Including test cases for new window types
4. Updating documentation with examples

## License

See project root for license information.

## References

- [RSP-QL Specification](https://streamreasoning.org/RSP-QL/)
- [RDF Stream Processing Community](https://www.w3.org/community/rsp/)
- [SPARQL 1.1 Query Language](https://www.w3.org/TR/sparql11-query/)

## Support

For issues, questions, or contributions, please refer to the main project repository.