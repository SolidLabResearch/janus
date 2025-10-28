# JANUS-QL: Extended RSP-QL with Historical Window Support

## Overview

JANUS-QL is an extension of RSP-QL (RDF Stream Processing Query Language) that adds support for querying both live streaming data and historical data through a unified query interface. The JanusQL parser separates a single JANUS-QL query into:

1. **RSP-QL Query**: For processing live streaming data
2. **SPARQL Queries**: For querying historical data from persistent storage

## Key Features

- **Live Windows**: Standard RSP-QL sliding/tumbling windows for real-time stream processing
- **Historical Sliding Windows**: Time-based windows with an offset from the current time
- **Historical Fixed Windows**: Windows with absolute start and end timestamps
- **Unified Syntax**: Write one query that spans both live and historical data

## Window Types

### 1. Live Sliding Window

Standard RSP-QL window for real-time stream processing.

```sparql
FROM NAMED WINDOW :liveWindow ON STREAM :accelStream [RANGE 1000 STEP 10]
```

- **RANGE**: Window width in milliseconds
- **STEP**: Slide interval in milliseconds
- **Type**: `live`

### 2. Live Tumbling Window

Non-overlapping windows where RANGE equals STEP.

```sparql
FROM NAMED WINDOW :tumblingLive ON STREAM :accelStream [RANGE 1000 STEP 1000]
```

### 3. Historical Sliding Window

Query historical data with a sliding window relative to the current time.

```sparql
FROM NAMED WINDOW :historicalSliding ON STREAM :historicalAccl [OFFSET 2000 RANGE 1000 STEP 100]
```

- **OFFSET**: Time offset from now (milliseconds ago)
- **RANGE**: Window width in milliseconds
- **STEP**: Slide interval for generating multiple windows
- **Type**: `historical-sliding`

### 4. Historical Fixed Window

Query a specific time range in historical data using absolute timestamps.

```sparql
FROM NAMED WINDOW :fixedHistorical ON STREAM :historicalAccl [START 1745857573 END 1738485859]
```

- **START**: Absolute start timestamp (Unix time)
- **END**: Absolute end timestamp (Unix time)
- **Type**: `historical-fixed`

### 5. Historical Tumbling Window

Non-overlapping historical windows where RANGE equals STEP.

```sparql
FROM NAMED WINDOW :tumblingHistorical ON STREAM :accelStream [OFFSET 2000 RANGE 1000 STEP 1000]
```

## Complete Example

```sparql
PREFIX :     <https://rsp.js/schema#>
REGISTER RStream :acclWindowing AS
SELECT *

FROM NAMED WINDOW :liveWindow ON STREAM :accelStream [RANGE 1000 STEP 10]
FROM NAMED WINDOW :historicalSliding ON STREAM :historicalAccl [OFFSET 2000 RANGE 1000 STEP 100]
FROM NAMED WINDOW :fixedHistorical ON STREAM :historicalAccl [START 1745857573 END 1738485859]
FROM NAMED WINDOW :tumblingLive ON STREAM :accelStream [RANGE 1000 STEP 1000]
FROM NAMED WINDOW :tumblingHistorical ON STREAM :accelStream [OFFSET 2000 RANGE 1000 STEP 1000] 
WHERE {
  WINDOW :liveWindow {
    ?s ?p ?o .
    ?s :timestamp ?timestamp .
  }
  WINDOW :historicalSliding {
    ?hs ?hp ?ho .
  }
}
```

## Parser Usage

### Basic Usage

```typescript
import { JanusQLParser } from './JanusQLParser';

const parser = new JanusQLParser();
const result = parser.parse(janusQLQuery);

// Access parsed components
console.log('Live Windows:', result.liveWindows);
console.log('Historical Windows:', result.historicalWindows);
console.log('RSP-QL Query:', result.rspqlQuery);
console.log('SPARQL Queries:', result.sparqlQueries);
```

### Parsed Query Structure

```typescript
interface ParsedJanusQuery {
  r2s: R2SOperator | null;              // REGISTER operator info
  liveWindows: WindowDefinition[];       // Live windows
  historicalWindows: WindowDefinition[]; // Historical windows
  rspqlQuery: string;                    // Generated RSP-QL for live data
  sparqlQueries: string[];               // Generated SPARQL for historical data
  prefixes: Map<string, string>;         // PREFIX mappings
  whereClause: string;                   // WHERE clause content
  selectClause: string;                  // SELECT clause
}
```

### Window Definition

```typescript
interface WindowDefinition {
  window_name: string;    // Fully expanded IRI
  stream_name: string;    // Fully expanded IRI
  width: number;          // RANGE value in milliseconds
  slide: number;          // STEP value in milliseconds
  offset?: number;        // OFFSET value (for historical sliding)
  start?: number;         // START timestamp (for historical fixed)
  end?: number;           // END timestamp (for historical fixed)
  type: 'live' | 'historical-sliding' | 'historical-fixed';
}
```

## Generated Queries

### RSP-QL Query (Live)

The parser generates a standard RSP-QL query containing only the live windows:

```sparql
PREFIX : <https://rsp.js/schema#>

REGISTER RStream :acclWindowing AS
SELECT *

FROM NAMED WINDOW :liveWindow ON STREAM :accelStream [RANGE 1000 STEP 10]
FROM NAMED WINDOW :tumblingLive ON STREAM :accelStream [RANGE 1000 STEP 1000]
WHERE {
  WINDOW :liveWindow {
    ?s ?p ?o .
    ?s :timestamp ?timestamp .
  }
}
```

### SPARQL Queries (Historical)

The parser generates separate SPARQL queries for each historical window:

```sparql
PREFIX : <https://rsp.js/schema#>

SELECT *

FROM NAMED :historicalSliding

WHERE {
  GRAPH :historicalSliding {
    ?hs ?hp ?ho .
  }
  # Historical sliding window: offset=2000, range=1000, step=100
}
```

## Implementation Details

### Parser Flow

1. **Line-by-line parsing**: Processes the query line by line
2. **Prefix extraction**: Captures and expands PREFIX declarations
3. **Window classification**: Identifies windows as live or historical based on syntax
4. **Query generation**: 
   - Creates one RSP-QL query with all live windows
   - Creates individual SPARQL queries for each historical window
5. **IRI handling**: Unwraps prefixed IRIs and re-wraps them as needed

### Key Methods

- `parse(query: string)`: Main entry point, returns ParsedJanusQuery
- `parseWindow(line: string, prefixes: Map<string, string>)`: Parses window definitions
- `generateRSPQLQuery(parsed, prefixLines)`: Generates RSP-QL for live windows
- `generateSPARQLQueries(parsed, prefixLines)`: Generates SPARQL for historical windows
- `unwrap(prefixedIri, mapper)`: Expands prefixed IRIs to full URIs
- `wrapIRI(iri, prefixes)`: Converts full URIs back to prefixed form

### Window Detection Logic

The parser uses regex patterns to distinguish window types:

1. **Historical Sliding**: Matches `[OFFSET ... RANGE ... STEP ...]`
2. **Historical Fixed**: Matches `[START ... END ...]`
3. **Live**: Matches `[RANGE ... STEP ...]` without OFFSET/START/END

## Use Cases

### 1. Real-time Monitoring with Historical Context

Monitor live sensor data while comparing against historical patterns:

```sparql
FROM NAMED WINDOW :current ON STREAM :sensors [RANGE 5000 STEP 1000]
FROM NAMED WINDOW :lastHour ON STREAM :sensors [OFFSET 3600000 RANGE 5000 STEP 1000]
```

### 2. Anomaly Detection

Compare current behavior against a known-good historical baseline:

```sparql
FROM NAMED WINDOW :live ON STREAM :metrics [RANGE 10000 STEP 1000]
FROM NAMED WINDOW :baseline ON STREAM :metrics [START 1700000000 END 1700086400]
```

### 3. Time-series Analysis

Analyze trends across different time periods:

```sparql
FROM NAMED WINDOW :recent ON STREAM :events [OFFSET 0 RANGE 60000 STEP 10000]
FROM NAMED WINDOW :hourAgo ON STREAM :events [OFFSET 3600000 RANGE 60000 STEP 10000]
FROM NAMED WINDOW :dayAgo ON STREAM :events [OFFSET 86400000 RANGE 60000 STEP 10000]
```

## Limitations and Future Work

### Current Limitations

- Historical SPARQL queries include comment-based metadata but don't automatically inject timestamp filters
- WHERE clause transformation is basic (WINDOW → GRAPH replacement)
- No support for multiple R2S operators per query

### Planned Enhancements

- Automatic timestamp variable injection and filtering in SPARQL queries
- Support for CONSTRUCT queries
- Window join optimization hints
- Temporal reasoning operators
- Aggregation functions across live and historical data

## References

- [RSP-QL Specification](https://streamreasoning.org/RSP-QL/)
- [RDF Stream Processing](https://www.w3.org/community/rsp/)
- [SPARQL 1.1 Query Language](https://www.w3.org/TR/sparql11-query/)