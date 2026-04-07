
## Known Limitations

### Replay Metrics
Currently, the `/api/replay/status` endpoint shows basic status (running/not running, elapsed time) but not detailed event counts. This is because `StreamBus` creates its own Tokio runtime which conflicts with the async HTTP server runtime.

**Workaround**: Check storage directory size or MQTT topic activity for progress indication.

**Future Fix**: Refactor `StreamBus` to accept an external runtime or use shared atomic counters.

