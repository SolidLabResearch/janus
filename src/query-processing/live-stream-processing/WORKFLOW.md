# JANUS-QL Processing Workflow

This document provides a visual representation of how JANUS-QL queries are processed from input to execution.

## Complete Workflow

```
┌───────────────────────────────────────────────────────────────────────┐
│                         JANUS-QL Query Input                          │
│                                                                       │
│  PREFIX : <https://rsp.js/schema#>                                   │
│  REGISTER RStream :output AS                                         │
│  SELECT *                                                            │
│  FROM NAMED WINDOW :live ON STREAM :s [RANGE 1000 STEP 10]         │
│  FROM NAMED WINDOW :hist ON STREAM :s [OFFSET 2000 RANGE 1000 ...]  │
│  WHERE { ... }                                                       │
└───────────────────────────────┬───────────────────────────────────────┘
                                │
                                ▼
                    ┌───────────────────────┐
                    │   JanusQLParser       │
                    │   .parse(query)       │
                    └───────────┬───────────┘
                                │
                ┌───────────────┴───────────────┐
                │    Line-by-Line Parsing       │
                │                               │
                │  1. Extract PREFIXes          │
                │  2. Parse REGISTER            │
                │  3. Extract SELECT            │
                │  4. Classify Windows          │
                │  5. Extract WHERE clause      │
                └───────────────┬───────────────┘
                                │
                    ┌───────────┴───────────┐
                    │   Window Detection    │
                    └───────────┬───────────┘
                                │
        ┌───────────────────────┼───────────────────────┐
        │                       │                       │
        ▼                       ▼                       ▼
┌───────────────┐    ┌──────────────────┐    ┌──────────────────┐
│ LIVE SLIDING  │    │ HISTORICAL       │    │ HISTORICAL       │
│               │    │ SLIDING          │    │ FIXED            │
│ [RANGE STEP]  │    │ [OFFSET RANGE    │    │ [START END]      │
│               │    │  STEP]           │    │                  │
└───────┬───────┘    └────────┬─────────┘    └────────┬─────────┘
        │                     │                       │
        │                     │                       │
        └──────────┬──────────┴──────────┬────────────┘
                   │                     │
        ┌──────────▼──────────┐  ┌───────▼────────┐
        │  Live Windows List  │  │  Historical    │
        │                     │  │  Windows List  │
        │  - :live (RANGE=    │  │                │
        │    1000, STEP=10)   │  │  - :hist       │
        └──────────┬──────────┘  │    (OFFSET=    │
                   │              │     2000...)   │
                   │              └───────┬────────┘
                   │                      │
        ┌──────────▼──────────┐  ┌───────▼────────────────────┐
        │  RSP-QL Generator   │  │  SPARQL Generator          │
        │                     │  │                            │
        │  - Add PREFIXes     │  │  - Add PREFIXes           │
        │  - Add REGISTER     │  │  - Add SELECT             │
        │  - Add SELECT       │  │  - Add FROM NAMED         │
        │  - Add live windows │  │  - Transform WHERE        │
        │  - Add WHERE        │  │    (WINDOW → GRAPH)       │
        └──────────┬──────────┘  │  - Add filters/metadata   │
                   │              └───────┬────────────────────┘
                   │                      │
        ┌──────────▼──────────┐  ┌───────▼────────────────────┐
        │  Generated RSP-QL   │  │  Generated SPARQL          │
        │  (1 query)          │  │  (N queries, 1 per window) │
        └──────────┬──────────┘  └───────┬────────────────────┘
                   │                      │
                   │                      │
        ┌──────────▼──────────┐  ┌───────▼────────────────────┐
        │   ParsedJanusQuery  │  │                            │
        │                     │  │  {                         │
        │  {                  │  │    r2s: {...},            │
        │    r2s: {...},      │  │    liveWindows: [...],    │
        │    liveWindows,     │  │    historicalWindows: [...]│
        │    historicalWindows│  │    rspqlQuery: "...",     │
        │    rspqlQuery,      │  │    sparqlQueries: [...]   │
        │    sparqlQueries,   │  │  }                        │
        │    ...              │  │                            │
        │  }                  │  │                            │
        └──────────┬──────────┘  └────────────────────────────┘
                   │
                   └──────────────┬──────────────┐
                                  │              │
                       ┌──────────▼───────┐  ┌───▼────────────┐
                       │  Application     │  │  Application   │
                       │  (Live Path)     │  │  (Hist Path)   │
                       └──────────┬───────┘  └───┬────────────┘
                                  │              │
                       ┌──────────▼───────┐  ┌───▼────────────┐
                       │   RSP Engine     │  │  SPARQL        │
                       │   (rsp-js)       │  │  Endpoint      │
                       │                  │  │  (Triplestore) │
                       │  - Process live  │  │                │
                       │    streams       │  │  - Query       │
                       │  - Emit results  │  │    historical  │
                       └──────────┬───────┘  │    data        │
                                  │          └───┬────────────┘
                                  │              │
                       ┌──────────▼──────────────▼────────────┐
                       │      Combined Results               │
                       │                                     │
                       │  - Real-time streaming data        │
                       │  - Historical context data         │
                       │  - Unified temporal view           │
                       └─────────────────────────────────────┘
```

## Window Classification Flow

```
┌──────────────────────────────────┐
│  FROM NAMED WINDOW Line          │
└────────────┬─────────────────────┘
             │
             ▼
    ┌────────────────────┐
    │  Regex Matching    │
    └────────┬───────────┘
             │
    ┌────────┴────────┐
    │                 │
    ▼                 ▼
┌─────────────┐  ┌──────────────────────┐
│ Contains    │  │ Contains             │
│ [OFFSET ... │  │ [START ... END ...]? │
│  RANGE ...  │  │                      │
│  STEP ...]? │  │                      │
└──┬──────────┘  └──┬───────────────────┘
   │ YES            │ YES
   ▼                ▼
┌─────────────┐  ┌──────────────────────┐
│ HISTORICAL  │  │ HISTORICAL           │
│ SLIDING     │  │ FIXED                │
└─────────────┘  └──────────────────────┘
   │ NO
   ▼
┌─────────────────────────┐
│ Contains                │
│ [RANGE ... STEP ...]?   │
└──────────┬──────────────┘
           │ YES
           ▼
    ┌──────────────┐
    │ LIVE         │
    │ (SLIDING or  │
    │  TUMBLING)   │
    └──────────────┘
```

## Data Structure Flow

```
┌─────────────────────────────────────────────────────────────┐
│                    ParsedJanusQuery                         │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ r2s: R2SOperator                                     │  │
│  │   - operator: "RStream" | "IStream" | "DStream"     │  │
│  │   - name: "https://example.org/output"              │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                             │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ liveWindows: WindowDefinition[]                      │  │
│  │   [{                                                 │  │
│  │     window_name: "https://example.org/live"         │  │
│  │     stream_name: "https://example.org/stream"       │  │
│  │     width: 1000,                                     │  │
│  │     slide: 10,                                       │  │
│  │     type: "live"                                     │  │
│  │   }]                                                 │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                             │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ historicalWindows: WindowDefinition[]                │  │
│  │   [{                                                 │  │
│  │     window_name: "https://example.org/hist"         │  │
│  │     stream_name: "https://example.org/stream"       │  │
│  │     width: 1000,                                     │  │
│  │     slide: 100,                                      │  │
│  │     offset: 2000,                                    │  │
│  │     type: "historical-sliding"                       │  │
│  │   }]                                                 │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                             │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ rspqlQuery: string (RSP-QL for live windows)        │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                             │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ sparqlQueries: string[] (1 per historical window)   │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                             │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ prefixes: Map<string, string>                        │  │
│  │   Map { "": "https://rsp.js/schema#" }              │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                             │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ selectClause: string                                 │  │
│  │ whereClause: string                                  │  │
│  └──────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

## Execution Flow

```
                    Application Start
                          │
                          ▼
                  ┌───────────────┐
                  │ Load JANUS-QL │
                  │ Query         │
                  └───────┬───────┘
                          │
                          ▼
                  ┌───────────────┐
                  │ Parse Query   │
                  │ (JanusQL      │
                  │  Parser)      │
                  └───┬───────────┘
                      │
        ┏━━━━━━━━━━━━━┻━━━━━━━━━━━━━┓
        ▼                            ▼
┌───────────────┐            ┌──────────────────┐
│ Setup Live    │            │ Setup Historical │
│ Processing    │            │ Queries          │
├───────────────┤            ├──────────────────┤
│               │            │                  │
│ 1. Initialize │            │ 1. Connect to    │
│    RSP Engine │            │    triplestore   │
│                │            │                  │
│ 2. Register   │            │ 2. Execute each  │
│    stream     │            │    SPARQL query  │
│    sources    │            │                  │
│                │            │ 3. Retrieve      │
│ 3. Start      │            │    historical    │
│    processing │            │    results       │
│                │            │                  │
│ 4. Listen for │            │ 4. Cache/store   │
│    results    │            │    results       │
└───────┬───────┘            └──────┬───────────┘
        │                           │
        │ Continuous Stream         │ One-time or
        │ of Results                │ Periodic Query
        │                           │
        ▼                           ▼
┌───────────────────────────────────────────────┐
│          Results Aggregation/Fusion           │
│                                               │
│  - Combine live and historical results       │
│  - Apply temporal reasoning                  │
│  - Generate insights                         │
│  - Trigger actions/alerts                    │
└───────────────────┬───────────────────────────┘
                    │
                    ▼
            ┌───────────────┐
            │ Application   │
            │ Output        │
            │               │
            │ - Dashboard   │
            │ - Alerts      │
            │ - API         │
            │ - Storage     │
            └───────────────┘
```

## Example Processing Steps

### Input Query
```sparql
PREFIX : <https://rsp.js/schema#>
REGISTER RStream :output AS
SELECT ?temp
FROM NAMED WINDOW :live ON STREAM :sensors [RANGE 1000 STEP 10]
FROM NAMED WINDOW :hist ON STREAM :sensors [OFFSET 3600000 RANGE 1000 STEP 1000]
WHERE {
  WINDOW :live { ?s :temperature ?temp }
}
```

### Step-by-Step Processing

1. **Parse**: Extract components
   - PREFIX: `:` → `https://rsp.js/schema#`
   - R2S: `RStream :output`
   - SELECT: `?temp`
   - Windows: 2 detected

2. **Classify Windows**
   - `:live` → LIVE (no OFFSET/START/END)
   - `:hist` → HISTORICAL-SLIDING (has OFFSET)

3. **Generate RSP-QL** (for `:live`)
   ```sparql
   PREFIX : <https://rsp.js/schema#>
   REGISTER RStream :output AS
   SELECT ?temp
   FROM NAMED WINDOW :live ON STREAM :sensors [RANGE 1000 STEP 10]
   WHERE {
     WINDOW :live { ?s :temperature ?temp }
   }
   ```

4. **Generate SPARQL** (for `:hist`)
   ```sparql
   PREFIX : <https://rsp.js/schema#>
   SELECT ?temp
   FROM NAMED :hist
   WHERE {
     GRAPH :live { ?s :temperature ?temp }
     # Historical sliding window: offset=3600000, range=1000, step=1000
   }
   ```

5. **Execute**
   - RSP Engine processes live stream
   - Triplestore returns historical data
   - Application combines results

## Integration Points

```
┌─────────────────┐
│ Your            │
│ Application     │
└────────┬────────┘
         │
         ├─────────► JanusQLParser.parse(query)
         │
         ▼
┌─────────────────────────────┐
│ ParsedJanusQuery            │
└────────┬─────────┬──────────┘
         │         │
         │         └──────────► result.sparqlQueries[]
         │                     │
         │                     ▼
         │              ┌─────────────────┐
         │              │ SPARQL Endpoint │
         │              │ (e.g., Virtuoso,│
         │              │  GraphDB, etc.) │
         │              └─────────────────┘
         │
         └──────────────► result.rspqlQuery
                         │
                         ▼
                  ┌─────────────────┐
                  │ RSP Engine      │
                  │ (rsp-js)        │
                  └─────────────────┘
```

## Error Handling Flow

```
┌──────────────────┐
│ Parse Query      │
└────────┬─────────┘
         │
         ▼
    ┌────────────┐
    │ Valid      │◄───────┐
    │ Syntax?    │        │
    └──┬─────┬───┘        │
       │ NO  │ YES        │
       ▼     ▼            │
    ┌─────┐ ┌──────────┐ │
    │Throw│ │Continue  │ │
    │Error│ │Parsing   │ │
    └─────┘ └────┬─────┘ │
                 │        │
                 ▼        │
           ┌──────────┐  │
           │ Window   │  │
           │ Valid?   │  │
           └──┬───┬───┘  │
              │ NO│ YES  │
              │   └──────┘
              ▼
         ┌─────────┐
         │ Return  │
         │ Partial │
         │ Results │
         └─────────┘
```

## Performance Considerations

```
Window Configuration Impact:

┌──────────────────────────────────────────────┐
│ RANGE (Window Size)                          │
│  ↑ Larger  = More data per window           │
│  ↓ Smaller = Less data, more frequent       │
└──────────────────────────────────────────────┘

┌──────────────────────────────────────────────┐
│ STEP (Slide Interval)                        │
│  ↑ Larger  = Less frequent updates           │
│  ↓ Smaller = More frequent, higher cost      │
└──────────────────────────────────────────────┘

┌──────────────────────────────────────────────┐
│ Tumbling (RANGE = STEP)                      │
│  = Best performance, no overlap              │
└──────────────────────────────────────────────┘

┌──────────────────────────────────────────────┐
│ Number of Windows                            │
│  ↑ More = Higher computational cost          │
│  Recommendation: Limit to 5-10 per query     │
└──────────────────────────────────────────────┘
```

---

This workflow provides a comprehensive view of how JANUS-QL queries are processed from input through parsing to execution across both live and historical data sources.