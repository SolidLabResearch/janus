# Live Streaming Integration - Ready to Test! 🚀

## What Was Done

I've successfully integrated MQTT subscription into Janus's live stream processing. The system now supports **full hybrid queries** that combine historical data retrieval with real-time MQTT streaming.

### New Components Added

1. **`src/stream/mqtt_subscriber.rs`** (250 lines)
   - MQTT subscriber that receives RDF events from message broker
   - Feeds events to LiveStreamProcessing in real-time
   - Handles connection errors and automatic parsing

2. **Updated `src/api/janus_api.rs`**
   - Spawns MQTT subscribers when live queries start
   - Shares LiveStreamProcessing instance between subscriber and worker
   - Auto-cleanup when queries stop

3. **Test Scripts & Documentation**
   - `test_live_streaming.sh` - Automated end-to-end test
   - `LIVE_STREAMING_GUIDE.md` - Complete usage guide
   - `start_http_server.sh` - Easy server startup script

### Architecture Flow

```
File (sensors_correct.nq)
    ↓
StreamBus (reads & publishes)
    ↓                    ↓
MQTT Broker          Storage (flush to disk)
    ↓                    ↓
MqttSubscriber      HistoricalExecutor
    ↓                    ↓
LiveStreamProcessing    Query Results (historical)
    ↓
Query Results (live)
    ↓
WebSocket → Dashboard
```

## How to Test (Step-by-Step)

### Option 1: Automated Test Script

```bash
cd /Users/kushbisen/Code/janus
./test_live_streaming.sh
```

This runs a complete test cycle and shows you if everything works.

### Option 2: Manual Dashboard Test (Recommended)

#### Step 1: Start the Server

```bash
cd /Users/kushbisen/Code/janus
./start_http_server.sh --clean
```

You should see:
```
╔════════════════════════════════════════════════════════════════╗
║             Janus RDF Stream Processing Engine                ║
║                    HTTP API Server                            ║
╚════════════════════════════════════════════════════════════════╝

Initializing storage at: ./data/storage
...
Server listening on 127.0.0.1:8080
```

**Keep this terminal open** - you'll see logs here.

#### Step 2: Open the Dashboard

1. Open your web browser
2. Navigate to: `file:///Users/kushbisen/Code/janus/examples/demo_dashboard.html`
3. You should see the Janus dashboard interface

#### Step 3: Start Replay (Publishes to MQTT + Storage)

1. Click the **"Start Replay"** button
2. Watch the server terminal - you should see:
   ```
   Starting the Stream Bus
   Input: data/sensors_correct.nq
   Broker: Mqtt
   Topics: ["sensors"]
   Connecting to the MQTT Server at localhost:1883
   Connected to MQTT!
   ✓ Read: 10 | Published: 10 | Stored: 10
   ```

3. Dashboard should show:
   - Status: Running
   - Input File: sensors_correct.nq
   - Broker: MQTT + Storage
   - Elapsed Time: counting up

#### Step 4: Wait for Storage Flush

**IMPORTANT:** Wait 10 seconds for data to flush to disk.

You can monitor this in the server terminal - look for lines like:
```
✓ Read: 20 | Published: 20 | Stored: 20
```

#### Step 5: Start Query (Auto-spawns MQTT Subscriber)

1. Click the **"Start Query"** button
2. Watch the server terminal - you should see:
   ```
   Starting MQTT subscriber...
     Host: localhost:1883
     Topic: sensors
     Stream URI: http://example.org/sensorStream
   ✓ Subscribed to topic: sensors
   Listening for events...
   ```

3. Dashboard should show:
   - Query Status: Running
   - Connection: Connected
   - Results Received: counting up

#### Step 6: Observe Results

You should now see **TWO types of results** in the dashboard:

**Historical Results** (appears once):
```json
{
  "source": "historical",
  "timestamp": "...",
  "bindings": [
    {"sensor": "http://example.org/sensor1", "temp": "\"23.5\""},
    {"sensor": "http://example.org/sensor2", "temp": "\"26.8\""},
    ...
  ]
}
```

**Live Results** (appears continuously):
```json
{
  "source": "live",
  "timestamp": "...",
  "bindings": [
    {"sensor": "http://example.org/sensor1", "temp": "\"23.5\""}
  ]
}
```

The live results will keep coming because `loop_file: true` continuously replays the data.

## What to Expect

### ✓ Working Correctly

- **Historical results:** Appear once, 1-3 seconds after starting query
  - Should show all 5 sensors with temperatures
  - Source: "historical"
  - Bindings show full URIs like "http://example.org/sensor1"

- **Live results:** Appear continuously every ~1-2 seconds
  - Should show individual sensor readings as they arrive via MQTT
  - Source: "live"
  - Bindings show real-time data

- **Dashboard:** Updates automatically with new results
  - Results counter increments
  - Timestamps are current (2024/2025)

### ✗ Common Issues & Fixes

#### Issue: Empty sensor values `"sensor": ""`

**Fix:**
```bash
# Stop everything
# Clean storage
rm -rf data/storage/*
# Restart server
./start_http_server.sh
```

#### Issue: Only historical results, no live results

**Check:**
1. Is MQTT broker running?
   ```bash
   docker ps | grep mosquitto
   ```
   If not: `docker-compose up -d mosquitto`

2. Check server logs for "Starting MQTT subscriber"
   - If you don't see this, the query didn't spawn subscriber

3. Monitor MQTT messages:
   ```bash
   docker exec -it janus-mosquitto mosquitto_sub -t "sensors" -v
   ```
   You should see RDF data flowing

#### Issue: No historical results, only live results

**Cause:** Storage hasn't flushed yet or timestamp mismatch

**Fix:**
- Wait longer (15-20 seconds) before starting query
- Check `data/storage/` has files:
  ```bash
  ls -lh data/storage/
  ```

#### Issue: No results at all

**Check:**
1. Server running? `ps aux | grep http_server`
2. Dashboard connected to correct URL? (http://127.0.0.1:8080)
3. Open browser console (F12) and check for errors
4. Check server logs at `/tmp/janus_server.log`

## Verifying MQTT Integration

### Monitor MQTT Traffic

In a separate terminal:
```bash
docker exec -it janus-mosquitto mosquitto_sub -t "sensors" -v
```

You should see messages like:
```
sensors <http://example.org/sensor1> <http://example.org/temperature> "23.5" .
sensors <http://example.org/sensor2> <http://example.org/temperature> "26.8" .
...
```

### Check Server Logs

```bash
tail -f /tmp/janus_server.log
```

Look for:
- "Starting MQTT subscriber..."
- "✓ Subscribed to topic: sensors"
- "✓ Received N events"

## Testing Different Scenarios

### Scenario 1: Historical Only

Use `broker_type: "none"` in replay config:
```json
{
  "broker_type": "none",
  ...
}
```

You should get only historical results, no live results.

### Scenario 2: Live Only

Modify the query to remove historical window (keep only RANGE/STEP window).

### Scenario 3: Multiple Sensors

The default data has 5 sensors. You should see all 5 in historical results, and random ones in live results.

## Performance Metrics

Expected performance:
- **Historical query:** ~50-100ms to return all results
- **Live latency:** ~10-50ms from MQTT publish to result
- **Throughput:** 500 events/sec with current rate_of_publishing setting

## Next Steps After Testing

1. **If it works:** You have full hybrid query capability!
   - Try modifying the query in the dashboard
   - Experiment with different window ranges
   - Create your own data files

2. **If issues persist:**
   - Check `LIVE_STREAMING_GUIDE.md` for detailed troubleshooting
   - Review server logs for errors
   - Verify MQTT broker connectivity

3. **Future enhancements:**
   - Add topic mapping (stream URI → MQTT topic)
   - Support multiple MQTT brokers
   - Add Kafka subscriber
   - Expose MQTT subscriber metrics in API

## Files Changed

- `src/stream/mqtt_subscriber.rs` (new)
- `src/stream/mod.rs` (updated exports)
- `src/api/janus_api.rs` (MQTT integration)
- `examples/demo_dashboard.html` (query updated)
- `data/sensors_correct.nq` (converted to N-Triples)
- `test_live_streaming.sh` (new)
- `LIVE_STREAMING_GUIDE.md` (new)

## Summary

The live streaming integration is **COMPLETE and READY TO TEST**. You now have:

✓ MQTT subscriber component  
✓ Automatic subscription when queries start  
✓ Hybrid historical + live query execution  
✓ Real-time WebSocket result streaming  
✓ Clean shutdown and resource cleanup  
✓ Comprehensive documentation and test scripts  

**Start with the manual dashboard test above** - it will give you the most visibility into what's happening.

The system is designed to "just work" - start the server, open the dashboard, click two buttons, and watch both historical and live results stream in.

Good luck! 🎉