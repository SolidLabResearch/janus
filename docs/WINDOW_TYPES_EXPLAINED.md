# JanusQL Window Types Explained

## Three Types of Queries

### 1. HISTORICAL ONLY
Returns ONLY past data from storage. No live updates.

```sparql
PREFIX ex: <http://example.org/>
REGISTER RStream ex:output AS
SELECT ?sensor ?temp
FROM NAMED WINDOW ex:histWindow ON STREAM ex:sensorStream [START 1 END 10000000]
WHERE {
  WINDOW ex:histWindow {
    ?sensor ex:temperature ?temp .
  }
}
```

**Use when:**
- Analyzing past data
- No need for real-time updates
- Testing storage/historical processing

**Replay config:**
```json
{
  "broker_type": "none",  // Just storage
  "loop_file": false      // Run once
}
```

**Results:**
- Source: "historical" only
- All results returned at once
- No new results after query completes

---

### 2. LIVE ONLY
Returns ONLY real-time streaming data. No historical data.

```sparql
PREFIX ex: <http://example.org/>
REGISTER RStream ex:output AS
SELECT ?sensor ?temp
FROM NAMED WINDOW ex:liveWindow ON STREAM ex:sensorStream [RANGE 5000 STEP 2000]
WHERE {
  WINDOW ex:liveWindow {
    ?sensor ex:temperature ?temp .
  }
}
```

**Window spec:**
- `[RANGE 5000 STEP 2000]` 
- RANGE = window size (5 seconds)
- STEP = slide interval (2 seconds)

**Use when:**
- Only care about current/future data
- Real-time monitoring
- No need for historical context

**Replay config:**
```json
{
  "broker_type": "mqtt",  // MUST use MQTT!
  "loop_file": true       // Keep streaming
}
```

**Results:**
- Source: "live" only
- Continuous stream of results
- New window every 2 seconds

---

### 3. HYBRID (Historical + Live)
Returns historical data FIRST, then switches to live streaming.

```sparql
PREFIX ex: <http://example.org/>
REGISTER RStream ex:output AS
SELECT ?sensor ?temp
FROM NAMED WINDOW ex:histWindow ON STREAM ex:sensorStream [START 1 END 10000000]
FROM NAMED WINDOW ex:liveWindow ON STREAM ex:sensorStream [RANGE 5000 STEP 2000]
WHERE {
  WINDOW ex:histWindow {
    ?sensor ex:temperature ?temp .
  }
  WINDOW ex:liveWindow {
    ?sensor ex:temperature ?temp .
  }
}
```

**Note:** TWO window definitions!

**Use when:**
- Want complete picture: past + present
- Dashboard showing history + real-time updates
- Analysis combining historical context with live data

**Replay config:**
```json
{
  "broker_type": "mqtt",  // MUST use MQTT for live part!
  "loop_file": true       // Keep streaming
}
```

**Results:**
- First: Source "historical" (all at once)
- Then: Source "live" (continuous stream)
- Both appear in same WebSocket stream

---

## Current Dashboard Setup

The dashboard now uses **HYBRID** mode:

```sparql
FROM NAMED WINDOW ex:histWindow ON STREAM ex:sensorStream [START 1 END 10000000]
FROM NAMED WINDOW ex:liveWindow ON STREAM ex:sensorStream [RANGE 5000 STEP 2000]
```

### What Happens:

1. Click "Start Replay"
   - Loads data to storage
   - Publishes to MQTT
   - Loops continuously

2. Click "Start Query"
   - **Phase 1 (Historical):** Reads from storage, returns all matching data
   - **Phase 2 (Live):** Subscribes to MQTT, streams new results every 2s

3. WebSocket shows both:
   ```json
   // Historical results
   {"source": "historical", ...}
   {"source": "historical", ...}
   
   // Then live results
   {"source": "live", ...}
   {"source": "live", ...}
   ```

## How to Test Each Type

### Test Historical Only

Dashboard query:
```sparql
FROM NAMED WINDOW ex:histWindow ON STREAM ex:sensorStream [START 1 END 10000000]
WHERE {
  WINDOW ex:histWindow {
    ?sensor ex:temperature ?temp .
  }
}
```

Replay:
```json
{"broker_type": "none", "loop_file": false}
```

### Test Live Only

Dashboard query:
```sparql
FROM NAMED WINDOW ex:liveWindow ON STREAM ex:sensorStream [RANGE 5000 STEP 2000]
WHERE {
  WINDOW ex:liveWindow {
    ?sensor ex:temperature ?temp .
  }
}
```

Replay:
```json
{"broker_type": "mqtt", "loop_file": true}
```

**Important:** Start query BEFORE replay for live!

### Test Hybrid (Current)

Dashboard query:
```sparql
FROM NAMED WINDOW ex:histWindow ON STREAM ex:sensorStream [START 1 END 10000000]
FROM NAMED WINDOW ex:liveWindow ON STREAM ex:sensorStream [RANGE 5000 STEP 2000]
WHERE {
  WINDOW ex:histWindow { ... }
  WINDOW ex:liveWindow { ... }
}
```

Replay:
```json
{"broker_type": "mqtt", "loop_file": true}
```

Order:
1. Start replay (loads historical + starts MQTT)
2. Wait 3 seconds
3. Start query (gets historical, then subscribes to live)

## Common Mistakes

### ❌ Live query without MQTT
```sparql
FROM NAMED WINDOW ex:liveWindow ON STREAM ... [RANGE ...]
```
```json
{"broker_type": "none"}  // WRONG! Live needs MQTT
```

### ❌ Only historical window but expecting live
```sparql
FROM NAMED WINDOW ex:histWindow ON STREAM ... [START ... END ...]
// No live window!
```
Result: Only historical, no live updates

### ❌ Start replay after query (for live)
```
1. Start query ← subscribes to MQTT
2. Start replay ← publishes to MQTT
```
✅ This works!

```
1. Start replay ← publishes and completes
2. Start query ← misses the data!
```
❌ This misses events (unless loop_file: true)

## Summary

**Window Type = Query Type:**
- 1 historical window = Historical only query
- 1 live window = Live only query  
- 2 windows (hist + live) = Hybrid query

**Current Dashboard:**
- ✅ Hybrid query (both windows)
- ✅ MQTT enabled  
- ✅ Looping replay
- ✅ Should get both historical AND live results!

**Test it:**
```bash
open examples/demo_dashboard.html
# Start Replay → Wait 3s → Start Query
# Watch for both "historical" and "live" in results!
```
