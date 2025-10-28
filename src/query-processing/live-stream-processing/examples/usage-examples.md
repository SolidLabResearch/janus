# JANUS-QL Usage Examples

Practical examples demonstrating how to use the JANUS-QL parser for common streaming and historical data scenarios.

## Table of Contents

1. [Basic Examples](#basic-examples)
2. [IoT Sensor Monitoring](#iot-sensor-monitoring)
3. [Financial Data Analysis](#financial-data-analysis)
4. [Social Media Analytics](#social-media-analytics)
5. [System Performance Monitoring](#system-performance-monitoring)
6. [Advanced Patterns](#advanced-patterns)

---

## Basic Examples

### Example 1: Simple Live Stream

Query only live data without historical context.

```typescript
import { JanusQLParser } from '../JanusQLParser';

const query = `
PREFIX sensor: <https://example.org/sensors/>
REGISTER RStream sensor:output AS
SELECT ?temp ?timestamp
FROM NAMED WINDOW sensor:tempWindow ON STREAM sensor:temperature [RANGE 5000 STEP 1000]
WHERE {
  WINDOW sensor:tempWindow {
    ?reading sensor:temperature ?temp .
    ?reading sensor:timestamp ?timestamp .
  }
}
`;

const parser = new JanusQLParser();
const result = parser.parse(query);

console.log('Live Windows:', result.liveWindows.length);
console.log('Historical Windows:', result.historicalWindows.length);
// Output: Live Windows: 1, Historical Windows: 0
```

### Example 2: Simple Historical Query

Query only historical data from a fixed time period.

```typescript
const query = `
PREFIX sensor: <https://example.org/sensors/>
REGISTER RStream sensor:output AS
SELECT ?temp ?timestamp
FROM NAMED WINDOW sensor:histWindow ON STREAM sensor:temperature [START 1700000000 END 1700086400]
WHERE {
  WINDOW sensor:histWindow {
    ?reading sensor:temperature ?temp .
    ?reading sensor:timestamp ?timestamp .
  }
}
`;

const result = parser.parse(query);
console.log('SPARQL Queries:', result.sparqlQueries.length);
// Output: SPARQL Queries: 1
```

### Example 3: Combined Live and Historical

Query both live and historical data in a single query.

```typescript
const query = `
PREFIX sensor: <https://example.org/sensors/>
REGISTER RStream sensor:comparison AS
SELECT ?liveTemp ?histTemp
FROM NAMED WINDOW sensor:live ON STREAM sensor:temp [RANGE 5000 STEP 1000]
FROM NAMED WINDOW sensor:yesterday ON STREAM sensor:temp [OFFSET 86400000 RANGE 5000 STEP 5000]
WHERE {
  WINDOW sensor:live {
    ?l sensor:temperature ?liveTemp .
  }
  WINDOW sensor:yesterday {
    ?h sensor:temperature ?histTemp .
  }
}
`;

const result = parser.parse(query);
console.log('RSP-QL generated for live data');
console.log('SPARQL generated for historical data');
```

---

## IoT Sensor Monitoring

### Example 4: Smart Home Temperature Control

Monitor current temperature and compare with historical patterns to optimize HVAC.

```sparql
PREFIX home: <https://smarthome.example.org/>
PREFIX sensor: <https://example.org/sensors/>

REGISTER RStream home:hvacControl AS
SELECT ?currentTemp ?historicalAvg ?room
WHERE {
  WINDOW home:currentReadings {
    ?reading a sensor:TemperatureSensor .
    ?reading sensor:value ?currentTemp .
    ?reading sensor:room ?room .
    ?reading sensor:timestamp ?timestamp .
  }
  
  WINDOW home:lastWeekPattern {
    ?histReading a sensor:TemperatureSensor .
    ?histReading sensor:value ?historicalAvg .
    ?histReading sensor:room ?room .
  }
  
  FILTER(?currentTemp > ?historicalAvg + 2)
}

FROM NAMED WINDOW home:currentReadings ON STREAM home:tempStream [RANGE 10000 STEP 2000]
FROM NAMED WINDOW home:lastWeekPattern ON STREAM home:tempStream [OFFSET 604800000 RANGE 10000 STEP 10000]
```

**Use Case**: Detect unusual temperature spikes by comparing current readings with the same time last week.

### Example 5: Industrial Equipment Vibration Analysis

Monitor machine vibration and detect anomalies using historical baseline.

```sparql
PREFIX factory: <https://factory.example.org/>
PREFIX sensor: <https://example.org/sensors/>

REGISTER RStream factory:vibrationAlert AS
SELECT ?machineId ?currentVibration ?baselineVibration
WHERE {
  WINDOW factory:liveVibration {
    ?sensor sensor:machineId ?machineId .
    ?sensor sensor:vibration ?currentVibration .
    ?sensor sensor:timestamp ?ts .
  }
  
  WINDOW factory:normalOperation {
    ?baseline sensor:machineId ?machineId .
    ?baseline sensor:vibration ?baselineVibration .
    ?baseline sensor:timestamp ?baselineTs .
  }
  
  FILTER(?currentVibration > ?baselineVibration * 1.3)
}

FROM NAMED WINDOW factory:liveVibration ON STREAM factory:sensorStream [RANGE 2000 STEP 500]
FROM NAMED WINDOW factory:normalOperation ON STREAM factory:sensorStream [START 1700000000 END 1700086400]
```

**Use Case**: Use a known-good baseline period to detect equipment degradation.

---

## Financial Data Analysis

### Example 6: Stock Price Monitoring

Compare live stock prices with multiple historical periods.

```sparql
PREFIX fin: <https://finance.example.org/>
PREFIX stock: <https://example.org/stocks/>

REGISTER RStream fin:priceAlert AS
SELECT ?symbol ?currentPrice ?price1hAgo ?price1dAgo
WHERE {
  WINDOW fin:live {
    ?quote stock:symbol ?symbol .
    ?quote stock:price ?currentPrice .
    ?quote stock:timestamp ?ts .
  }
  
  WINDOW fin:oneHourAgo {
    ?q1h stock:symbol ?symbol .
    ?q1h stock:price ?price1hAgo .
  }
  
  WINDOW fin:oneDayAgo {
    ?q1d stock:symbol ?symbol .
    ?q1d stock:price ?price1dAgo .
  }
}

FROM NAMED WINDOW fin:live ON STREAM fin:quotes [RANGE 1000 STEP 100]
FROM NAMED WINDOW fin:oneHourAgo ON STREAM fin:quotes [OFFSET 3600000 RANGE 1000 STEP 1000]
FROM NAMED WINDOW fin:oneDayAgo ON STREAM fin:quotes [OFFSET 86400000 RANGE 1000 STEP 1000]
```

**Use Case**: Multi-timeframe analysis for trading algorithms.

### Example 7: Transaction Fraud Detection

Detect suspicious transactions by comparing with user's historical behavior.

```sparql
PREFIX bank: <https://bank.example.org/>
PREFIX txn: <https://example.org/transactions/>

REGISTER RStream bank:fraudAlert AS
SELECT ?userId ?amount ?location ?avgAmount
WHERE {
  WINDOW bank:currentTxn {
    ?transaction txn:userId ?userId .
    ?transaction txn:amount ?amount .
    ?transaction txn:location ?location .
    ?transaction txn:timestamp ?ts .
  }
  
  WINDOW bank:normalBehavior {
    ?histTxn txn:userId ?userId .
    ?histTxn txn:amount ?avgAmount .
    ?histTxn txn:timestamp ?histTs .
  }
  
  FILTER(?amount > ?avgAmount * 5)
}

FROM NAMED WINDOW bank:currentTxn ON STREAM bank:transactions [RANGE 5000 STEP 1000]
FROM NAMED WINDOW bank:normalBehavior ON STREAM bank:transactions [OFFSET 2592000000 RANGE 86400000 STEP 86400000]
```

**Use Case**: Compare current transaction with last 30 days of user behavior.

---

## Social Media Analytics

### Example 8: Trending Topics Detection

Identify topics trending now compared to historical baseline.

```sparql
PREFIX social: <https://social.example.org/>
PREFIX topic: <https://example.org/topics/>

REGISTER RStream social:trendingNow AS
SELECT ?topic ?currentMentions ?normalMentions
WHERE {
  WINDOW social:live {
    ?post topic:mentions ?topic .
    ?post social:timestamp ?ts .
  }
  
  WINDOW social:baseline {
    ?histPost topic:mentions ?topic .
    ?histPost social:timestamp ?histTs .
  }
}

FROM NAMED WINDOW social:live ON STREAM social:posts [RANGE 60000 STEP 10000]
FROM NAMED WINDOW social:baseline ON STREAM social:posts [OFFSET 604800000 RANGE 60000 STEP 60000]
```

**Use Case**: Compare current mention rate with average from last week.

### Example 9: User Engagement Tracking

Monitor real-time engagement and compare with campaign periods.

```sparql
PREFIX analytics: <https://analytics.example.org/>
PREFIX engagement: <https://example.org/engagement/>

REGISTER RStream analytics:campaignPerformance AS
SELECT ?userId ?currentActivity ?campaignActivity
WHERE {
  WINDOW analytics:realtime {
    ?event engagement:userId ?userId .
    ?event engagement:type ?eventType .
    ?event engagement:timestamp ?ts .
  }
  
  WINDOW analytics:lastCampaign {
    ?pastEvent engagement:userId ?userId .
    ?pastEvent engagement:type ?eventType .
    ?pastEvent engagement:timestamp ?pastTs .
  }
}

FROM NAMED WINDOW analytics:realtime ON STREAM analytics:events [RANGE 30000 STEP 5000]
FROM NAMED WINDOW analytics:lastCampaign ON STREAM analytics:events [START 1700000000 END 1700604800]
```

**Use Case**: Compare current user engagement with a successful past campaign.

---

## System Performance Monitoring

### Example 10: CPU Usage Anomaly Detection

Monitor CPU usage and detect anomalies compared to normal operation.

```sparql
PREFIX sys: <https://system.example.org/>
PREFIX metric: <https://example.org/metrics/>

REGISTER RStream sys:cpuAlert AS
SELECT ?hostname ?currentCpu ?normalCpu
WHERE {
  WINDOW sys:current {
    ?host metric:hostname ?hostname .
    ?host metric:cpuUsage ?currentCpu .
    ?host metric:timestamp ?ts .
  }
  
  WINDOW sys:normal {
    ?histHost metric:hostname ?hostname .
    ?histHost metric:cpuUsage ?normalCpu .
    ?histHost metric:timestamp ?histTs .
  }
  
  FILTER(?currentCpu > 80 && ?normalCpu < 50)
}

FROM NAMED WINDOW sys:current ON STREAM sys:metrics [RANGE 5000 STEP 1000]
FROM NAMED WINDOW sys:normal ON STREAM sys:metrics [OFFSET 86400000 RANGE 3600000 STEP 3600000]
```

**Use Case**: Alert when CPU usage is high now but was normal yesterday.

### Example 11: Network Traffic Analysis

Analyze current network patterns against multiple historical baselines.

```sparql
PREFIX net: <https://network.example.org/>
PREFIX traffic: <https://example.org/traffic/>

REGISTER RStream net:trafficAnalysis AS
SELECT ?interface ?currentBandwidth ?peak ?offPeak
WHERE {
  WINDOW net:live {
    ?iface traffic:interface ?interface .
    ?iface traffic:bandwidth ?currentBandwidth .
    ?iface traffic:timestamp ?ts .
  }
  
  WINDOW net:peakHours {
    ?peak traffic:interface ?interface .
    ?peak traffic:bandwidth ?peakBandwidth .
  }
  
  WINDOW net:offPeakHours {
    ?offPeak traffic:interface ?interface .
    ?offPeak traffic:bandwidth ?offPeakBandwidth .
  }
}

FROM NAMED WINDOW net:live ON STREAM net:packets [RANGE 10000 STEP 1000]
FROM NAMED WINDOW net:peakHours ON STREAM net:packets [START 1700046000 END 1700064000]
FROM NAMED WINDOW net:offPeakHours ON STREAM net:packets [START 1700010000 END 1700028000]
```

**Use Case**: Compare current traffic with both peak and off-peak historical periods.

---

## Advanced Patterns

### Example 12: Multi-Stream Correlation

Correlate data from multiple streams across live and historical windows.

```sparql
PREFIX app: <https://application.example.org/>
PREFIX logs: <https://example.org/logs/>
PREFIX metrics: <https://example.org/metrics/>

REGISTER RStream app:correlation AS
SELECT ?service ?errorRate ?responseTime ?historicalErrors
WHERE {
  WINDOW app:liveErrors {
    ?error logs:service ?service .
    ?error logs:level "ERROR" .
    ?error logs:timestamp ?ts .
  }
  
  WINDOW app:liveMetrics {
    ?metric metrics:service ?service .
    ?metric metrics:responseTime ?responseTime .
    ?metric metrics:timestamp ?mts .
  }
  
  WINDOW app:historicalErrors {
    ?histError logs:service ?service .
    ?histError logs:level "ERROR" .
    ?histError logs:timestamp ?hts .
  }
}

FROM NAMED WINDOW app:liveErrors ON STREAM app:errorStream [RANGE 30000 STEP 5000]
FROM NAMED WINDOW app:liveMetrics ON STREAM app:metricStream [RANGE 30000 STEP 5000]
FROM NAMED WINDOW app:historicalErrors ON STREAM app:errorStream [OFFSET 604800000 RANGE 30000 STEP 30000]
```

**Use Case**: Correlate error rates with performance metrics and historical patterns.

### Example 13: Sliding Window Comparison Chain

Compare current data with multiple sliding historical windows.

```sparql
PREFIX analytics: <https://analytics.example.org/>
PREFIX event: <https://example.org/events/>

REGISTER RStream analytics:trendAnalysis AS
SELECT ?eventType ?now ?ago1h ?ago6h ?ago24h
WHERE {
  WINDOW analytics:now {
    ?e event:type ?eventType .
    ?e event:timestamp ?ts .
  }
  
  WINDOW analytics:hour1 {
    ?e1 event:type ?eventType .
  }
  
  WINDOW analytics:hour6 {
    ?e6 event:type ?eventType .
  }
  
  WINDOW analytics:hour24 {
    ?e24 event:type ?eventType .
  }
}

FROM NAMED WINDOW analytics:now ON STREAM analytics:events [RANGE 60000 STEP 10000]
FROM NAMED WINDOW analytics:hour1 ON STREAM analytics:events [OFFSET 3600000 RANGE 60000 STEP 10000]
FROM NAMED WINDOW analytics:hour6 ON STREAM analytics:events [OFFSET 21600000 RANGE 60000 STEP 10000]
FROM NAMED WINDOW analytics:hour24 ON STREAM analytics:events [OFFSET 86400000 RANGE 60000 STEP 10000]
```

**Use Case**: Multi-horizon trend analysis for predictive analytics.

### Example 14: Mixed Tumbling and Sliding Windows

Combine tumbling windows for aggregations with sliding windows for real-time monitoring.

```sparql
PREFIX monitor: <https://monitor.example.org/>
PREFIX data: <https://example.org/data/>

REGISTER RStream monitor:hybrid AS
SELECT ?metric ?slidingValue ?tumblingValue
WHERE {
  WINDOW monitor:sliding {
    ?s data:metric ?metric .
    ?s data:value ?slidingValue .
    ?s data:timestamp ?ts .
  }
  
  WINDOW monitor:tumbling {
    ?t data:metric ?metric .
    ?t data:value ?tumblingValue .
    ?t data:timestamp ?tts .
  }
  
  WINDOW monitor:histTumbling {
    ?ht data:metric ?metric .
    ?ht data:value ?histValue .
  }
}

FROM NAMED WINDOW monitor:sliding ON STREAM monitor:stream [RANGE 5000 STEP 500]
FROM NAMED WINDOW monitor:tumbling ON STREAM monitor:stream [RANGE 10000 STEP 10000]
FROM NAMED WINDOW monitor:histTumbling ON STREAM monitor:stream [OFFSET 3600000 RANGE 10000 STEP 10000]
```

**Use Case**: Real-time monitoring with sliding windows + periodic aggregations with tumbling windows.

---

## TypeScript Integration Examples

### Example 15: Using Parser Results

```typescript
import { JanusQLParser, ParsedJanusQuery } from '../JanusQLParser';

async function processJanusQuery(query: string) {
  const parser = new JanusQLParser();
  const result: ParsedJanusQuery = parser.parse(query);
  
  // Process live windows with RSP engine
  if (result.rspqlQuery) {
    console.log('Initializing RSP engine with query:');
    console.log(result.rspqlQuery);
    // Initialize your RSP engine here
  }
  
  // Process historical windows with SPARQL endpoint
  for (let i = 0; i < result.sparqlQueries.length; i++) {
    const sparqlQuery = result.sparqlQueries[i];
    const window = result.historicalWindows[i];
    
    console.log(`\nQuerying historical data for window: ${window.window_name}`);
    console.log(`Type: ${window.type}`);
    
    if (window.type === 'historical-fixed') {
      console.log(`Period: ${new Date(window.start! * 1000)} to ${new Date(window.end! * 1000)}`);
    } else if (window.type === 'historical-sliding') {
      console.log(`Offset: ${window.offset}ms, Range: ${window.width}ms`);
    }
    
    // Execute SPARQL query against your triplestore
    // const historicalData = await sparqlEndpoint.query(sparqlQuery);
  }
  
  // Access metadata
  console.log('\nPrefixes:', Array.from(result.prefixes.entries()));
  console.log('R2S Operator:', result.r2s);
  console.log('Live Windows:', result.liveWindows.length);
  console.log('Historical Windows:', result.historicalWindows.length);
}
```

### Example 16: Building a Query Dynamically

```typescript
function buildTemperatureQuery(
  liveRange: number,
  liveStep: number,
  historicalOffset: number
): string {
  return `
    PREFIX sensor: <https://example.org/sensors/>
    REGISTER RStream sensor:tempAnalysis AS
    SELECT ?current ?historical
    FROM NAMED WINDOW sensor:live ON STREAM sensor:temp [RANGE ${liveRange} STEP ${liveStep}]
    FROM NAMED WINDOW sensor:hist ON STREAM sensor:temp [OFFSET ${historicalOffset} RANGE ${liveRange} STEP ${liveStep}]
    WHERE {
      WINDOW sensor:live {
        ?s sensor:temperature ?current .
      }
      WINDOW sensor:hist {
        ?h sensor:temperature ?historical .
      }
    }
  `;
}

const query = buildTemperatureQuery(5000, 1000, 3600000);
const parser = new JanusQLParser();
const result = parser.parse(query);
```

---

## Best Practices

1. **Window Sizing**: Choose RANGE and STEP values based on your data velocity and latency requirements
2. **Historical Offset**: Use realistic offsets - don't query too far back if data patterns change
3. **Fixed Windows**: Use for known events or baseline periods
4. **Tumbling Windows**: Use when you need non-overlapping aggregations
5. **Prefix Management**: Define all prefixes at the top for clarity
6. **WHERE Clause**: Structure to clearly separate live and historical patterns
7. **Filtering**: Apply filters to reduce data volume before window operations

## Performance Tips

- Use tumbling windows (RANGE = STEP) when you don't need overlapping windows
- Limit the number of historical windows per query
- Choose appropriate STEP values to avoid excessive computation
- Use fixed historical windows when the time period is known and doesn't change
- Consider the trade-off between window size and result freshness

---

## Common Patterns Summary

| Pattern | Live Windows | Historical Windows | Use Case |
|---------|--------------|-------------------|----------|
| Live Only | 1+ | 0 | Real-time monitoring |
| Historical Only | 0 | 1+ | Historical analysis |
| Comparison | 1 | 1 | Current vs past |
| Multi-horizon | 1 | 2+ | Trend analysis |
| Baseline | 1 | 1 fixed | Anomaly detection |
| Multi-stream | 2+ live | 1+ | Correlation analysis |

---

For more information, see the main [JANUS-QL documentation](../JANUS-QL.md).