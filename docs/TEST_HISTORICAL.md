# Testing Historical Queries - Quick Guide

## Your Data Has Timestamps Around 1-2 Million (Jan 1970)

When you see "1/21/1970, 3:08:09 AM", that's timestamp ~1,800,000 milliseconds.

## Fix 1: Use Matching Timestamp Range

Your query needs:
```sparql
[START 1 END 10000000]
```

This covers 0 to ~3 hours (10 million milliseconds = ~2.7 hours)

## Fix 2: For Historical Only, Use broker_type: "none"

MQTT errors happen because the replay completes before MQTT client fully connects.

For historical testing, use:
```json
{
  "broker_type": "none"  // Just stores to disk, no MQTT
}
```

## Updated Dashboard

The dashboard now uses:
- Timestamp range: `[START 1 END 10000000]` ✅
- Broker type: `"none"` ✅  
- No looping (completes quickly) ✅

## Test Steps

1. **Kill any existing server**
```bash
killall http_server 2>/dev/null
```

2. **Clear old storage**
```bash
rm -rf data/storage/*
```

3. **Start server**
```bash
cargo run --bin http_server
```

4. **Open dashboard**
```bash
open examples/demo_dashboard.html
```

5. **Click "Start Replay"**
- Should complete quickly
- No MQTT errors
- Data goes to storage

6. **Wait 3 seconds** (for flush)

7. **Click "Start Query"**
- Should get historical results!
- Check WebSocket panel for results

## Expected Results

You should see results like:
```json
{
  "query_id": "demo_query",
  "timestamp": 1800000,
  "source": "historical",
  "bindings": [
    {
      "sensor": "http://example.org/sensor1",
      "temp": "23.5"
    }
  ]
}
```

## If Still No Results

Check storage was created:
```bash
ls -lh data/storage/
# Should see segment files
```

Check the query is using the right predicate:
```bash
# Data has:
<...sensor1> <http://example.org/temperature> "23.5" ...

# Query must use:
?sensor ex:temperature ?temp
# OR
?sensor <http://example.org/temperature> ?temp
```

## For Live Processing (MQTT)

If you want live processing later:
1. Ensure MQTT is running: `docker ps | grep mosquitto`
2. Use `"broker_type": "mqtt"`
3. Add `"loop_file": true` for continuous stream
4. Register LIVE query (not historical):

```sparql
PREFIX ex: <http://example.org/>
REGISTER RStream ex:output AS
SELECT ?sensor ?temp
FROM NAMED WINDOW ex:liveWindow ON STREAM ex:sensorStream [RANGE 5000 STEP 1000]
WHERE {
  WINDOW ex:liveWindow {
    ?sensor ex:temperature ?temp .
  }
}
```

## Current Setup (Historical Only)

✅ broker_type: "none" - No MQTT needed  
✅ Timestamp range: [START 1 END 10000000] - Matches your data  
✅ Quick replay - No looping  
✅ Should work immediately!  

Try it now!
