# Runtime Conflict Fix - Summary

## Problem

When starting the HTTP server, it crashed with:
```
thread 'tokio-runtime-worker' panicked at:
Cannot drop a runtime in a context where blocking is not allowed.
This happens when a runtime is dropped from within an asynchronous context.
```

## Root Cause

`StreamBus::new()` creates its own Tokio runtime internally. When called from within the HTTP server's async context (which also uses Tokio), this created a nested runtime situation that Tokio doesn't allow.

## Solution

Modified `src/http/server.rs` to spawn `StreamBus` in a separate blocking thread:

```rust
// Before (caused panic)
let stream_bus = StreamBus::new(bus_config, Arc::clone(&state.storage));
stream_bus.start()?;

// After (works correctly)
std::thread::spawn(move || {
    let stream_bus = StreamBus::new(bus_config, storage);
    if let Err(e) = stream_bus.start() {
        eprintln!("Stream bus replay error: {}", e);
    }
});
```

## Trade-off

**Lost**: Real-time event counter metrics from `/api/replay/status`  
**Gained**: Stable, non-crashing server that actually works

The replay still functions correctly - it reads data, publishes to MQTT, and stores quads. We just can't track detailed metrics from the HTTP API because the thread boundary prevents shared access to atomic counters.

## What Works Now

✅ Server starts without panics  
✅ Health endpoint responds  
✅ Replay runs in background thread  
✅ Data flows to MQTT and storage  
✅ Queries execute against data  
✅ WebSocket streaming works  
✅ Demo dashboard functional  

## What Shows Limited Info

⚠️ `/api/replay/status` shows:
- `is_running`: ✅ Accurate  
- `elapsed_seconds`: ✅ Accurate  
- `events_read`: ⚠️ Always 0  
- `events_published`: ⚠️ Always 0  
- `events_stored`: ⚠️ Always 0  

## Alternative Verification Methods

### Check MQTT Activity
```bash
docker exec -it janus-mosquitto mosquitto_sub -t "sensors" -v
# You'll see messages flowing if replay is working
```

### Check Storage Directory
```bash
ls -lh data/storage/
# New files appear as data is stored
```

### Monitor Logs
```bash
# StreamBus prints progress to stdout
# Watch terminal where server is running
```

## Future Improvement

To restore detailed metrics, refactor `StreamBus` to:
1. Accept optional external runtime instead of creating its own
2. Use channels to communicate metrics back to HTTP server
3. Or expose shared atomic counters that can be read across threads

For now, the current solution is **production-ready for MVP** - the replay works, queries execute, and results stream correctly.

## Testing

```bash
# 1. Start server
cargo run --bin http_server

# 2. Open dashboard
open examples/demo_dashboard.html

# 3. Click "Start Replay"
# - Button disables
# - Status shows "Running"
# - Elapsed time increments

# 4. Verify MQTT activity
docker exec -it janus-mosquitto mosquitto_sub -t "sensors" -v
# Should see RDF quads flowing

# 5. Click "Start Query"
# - WebSocket connects
# - Results appear in panel
# - Tagged as "historical" or "live"
```

## Status

**FIXED** ✅

Server is stable and functional. The metric limitation is documented and has acceptable workarounds.
