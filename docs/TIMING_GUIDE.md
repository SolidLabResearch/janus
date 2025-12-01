# Timing Guide: When to Start Query After Replay

## TL;DR
**Wait 5-10 seconds** between "Start Replay" and "Start Query"

## Why the Wait?

### What Happens During Replay

1. **Read file** (instant)
2. **Write to storage** (buffered in memory)
3. **Publish to MQTT** (if enabled)
4. **Background flush to disk** (happens asynchronously)

The critical step is **#4 - Background Flush**

### Storage Flush Timing

From your server config:
```
--flush-interval-ms 5000     (5 seconds)
--max-batch-size-bytes 10485760  (10 MB)
```

**Flush happens when EITHER:**
- 5 seconds elapsed (flush interval)
- OR batch reaches 10 MB
- OR max events reached

For your small test file (6 lines), it will flush after **5 seconds**.

## Recommended Timing

### For Small Test Files (<100 events)
```
Start Replay
    ↓
Wait 5-10 seconds  ← Storage flush completes
    ↓
Start Query        ← Historical data is ready
```

### For Large Files (>1000 events)
```
Start Replay
    ↓
Wait 2-3 seconds   ← First batch flushes (size-based)
    ↓
Start Query        ← Some historical available, more coming
```

### For Continuous Streaming (loop_file: true)
```
Start Replay
    ↓
Wait 5-10 seconds  ← First batch flushed
    ↓
Start Query        ← Gets initial historical, then live
```

## How to Know It's Ready

### Check Server Logs
Look for messages like:
```
Flushed batch: X events
Segment created: ...
```

### Check Storage Directory
```bash
ls -lh data/storage/
# Should see files appear after ~5 seconds
```

### Check File Sizes
```bash
watch -n 1 'ls -lh data/storage/'
# Watch files appear/grow
```

## Current Dashboard Setup

With your config:
- Small file: 6 lines
- Flush interval: 5 seconds
- No loop (completes quickly)

**Optimal timing:**
```
1. Click "Start Replay"
2. Count to 8 (or watch for "Replay completed")
3. Click "Start Query"
```

## Visual Timing Guide

```
Time (seconds)   What's Happening
0                Click "Start Replay"
0.1              File read complete
0.2              All events in buffer
0.5              Publishing to MQTT (if enabled)
1.0              Replay loop iteration
...
5.0              ← FLUSH TRIGGERED (interval elapsed)
5.5              Segment file written to disk
6.0              ← SAFE TO START QUERY
```

## Why Historical Needs This Wait

Historical queries read from **disk storage**, not memory buffer.

```
Memory Buffer → Background Thread → Disk Storage
  (instant)      (every 5 seconds)    (queryable)
```

The query can't see data until it's flushed to disk!

## Live Queries Don't Need Wait

Live queries read from **MQTT**, not storage.

```
Replay → MQTT → Live Query
  ↓       ↓        ↓
Fast    Fast     Fast
```

**For live-only queries:**
```
1. Start Query (subscribes to MQTT)
2. Start Replay (publishes to MQTT)
3. Results appear immediately
```

## Hybrid Queries (Historical + Live)

Current dashboard setup needs wait for historical part:

```
1. Click "Start Replay"
   ↓
2. Wait 5-10 seconds (for historical flush)
   ↓
3. Click "Start Query"
   ↓
   → Historical results (from disk)
   → Then live results (from MQTT)
```

## Automatic Detection (Future Enhancement)

Could add to dashboard:
```javascript
// Check if storage has data
async function isStorageReady() {
  const status = await fetch('/api/replay/status');
  const data = await status.json();
  return data.elapsed_seconds > 6;
}

// Enable "Start Query" button only when ready
setInterval(async () => {
  if (await isStorageReady()) {
    enableQueryButton();
  }
}, 1000);
```

But for now, manual 5-10 second wait works fine.

## Quick Reference

| Scenario | Wait Time | Reason |
|----------|-----------|--------|
| Small file + historical | 5-10 sec | Flush interval |
| Large file + historical | 2-3 sec | Size-based flush |
| Live only | 0 sec | Reads from MQTT |
| Hybrid | 5-10 sec | Historical needs flush |
| Empty/test | 5 sec | Minimum flush interval |

## Your Current Setup

File: `data/sensors_correct.nq` (6 lines = ~500 bytes)
Config: Flush every 5 seconds OR 10 MB

**Recommended:**
```
Start Replay → Count to 8 → Start Query
```

This ensures:
- ✅ File read complete
- ✅ Buffer filled
- ✅ Background flush triggered
- ✅ Segment written to disk
- ✅ Historical data queryable
- ✅ MQTT streaming active

## Test Script

```bash
#!/bin/bash
echo "Starting replay..."
# Click "Start Replay" in dashboard

echo "Waiting for storage flush..."
for i in {8..1}; do
  echo "$i..."
  sleep 1
done

echo "Storage should be ready!"
echo "Click 'Start Query' now"
```

**Bottom line: Wait 8-10 seconds to be safe!** ⏱️
