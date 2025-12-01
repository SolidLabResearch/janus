# Janus HTTP API - START HERE

## Quick Start (30 seconds)

```bash
# 1. Setup (one time)
./test_setup.sh

# 2. Start MQTT
docker-compose up -d mosquitto

# 3. Start Server
cargo run --bin http_server

# 4. Open Dashboard
open examples/demo_dashboard.html
```

Then click: **Start Replay** → **Start Query**

## What This Does

1. **Start Replay**: Loads RDF data from `data/sensors.nq`, publishes to MQTT, stores locally
2. **Start Query**: Executes a JanusQL query, streams results via WebSocket to dashboard

## Documentation

- **QUICK_REFERENCE.md** - One-page cheat sheet
- **RUNTIME_FIX_SUMMARY.md** - How the runtime issue was fixed
- **COMPLETE_SOLUTION.md** - Full implementation details
- **SETUP_GUIDE.md** - Detailed setup instructions
- **README_HTTP_API.md** - Complete API documentation
- **FINAL_TEST.md** - Verification steps

## Key Points

✅ **No more runtime panics** - Fixed by spawning StreamBus in separate thread  
✅ **Correct JanusQL syntax** - All examples updated to match parser  
✅ **MQTT integration** - Full broker setup with Docker Compose  
✅ **Two-button demo** - Interactive dashboard for easy testing  
✅ **Production-ready** - Stable, tested, documented  

⚠️ **Known limitation**: Replay metrics show status but not event counts (acceptable trade-off)

## Troubleshooting

```bash
# Server won't start (port in use)
lsof -ti:8080 | xargs kill -9

# MQTT not running
docker-compose up -d mosquitto

# Check if working
curl http://localhost:8080/health
```

## Success Indicators

When everything works correctly:
1. Server starts with clean output (no panics)
2. Dashboard shows "Connected to Janus HTTP API server"  
3. Replay button → Status changes to "Running"
4. Query button → WebSocket connects, results appear
5. Results tagged as "historical" or "live"

## Need Help?

1. Read **QUICK_REFERENCE.md** for common commands
2. Check **FINAL_TEST.md** for verification steps
3. See **RUNTIME_FIX_SUMMARY.md** if you see panics
4. Review **SETUP_GUIDE.md** for detailed instructions

---

**Everything is ready. Just run the Quick Start commands above!** 🚀
