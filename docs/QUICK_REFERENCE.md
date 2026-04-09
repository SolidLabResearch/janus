# Janus HTTP API - Quick Reference

## Setup (3 Commands)

```bash
./scripts/test_setup.sh            # Optional local setup helper
docker-compose up -d mosquitto     # Start MQTT
cargo run --bin http_server        # Start server
```

## Local Demo Dashboard

```bash
open examples/demo_dashboard.html
```

For the maintained frontend, use:

- `https://github.com/SolidLabResearch/janus-dashboard`

## API Endpoints

```bash
# Health
GET http://localhost:8080/health

# Queries
POST   /api/queries              # Register
GET    /api/queries              # List all
GET    /api/queries/:id          # Details
POST   /api/queries/:id/start    # Start
POST   /api/queries/:id/stop     # Stop
DELETE /api/queries/:id          # Delete stopped query
WS     /api/queries/:id/results  # Stream

# Replay
POST /api/replay/start    # Start
POST /api/replay/stop     # Stop
GET  /api/replay/status   # Status
```

## JanusQL Syntax

```sparql
PREFIX ex: <http://example.org/>
REGISTER RStream ex:output AS
SELECT ?vars
FROM NAMED WINDOW ex:name ON STREAM ex:stream [WINDOW_SPEC]
WHERE {
  WINDOW ex:name {
    # SPARQL patterns
  }
}
```

### Window Specs

```sparql
[START 1704067200 END 1735689599]         # Historical fixed
[OFFSET 1704067200 RANGE 10000 STEP 2000] # Historical sliding
[RANGE 10000 STEP 5000]                   # Live sliding
```

## cURL Examples

### Register Query
```bash
curl -X POST http://localhost:8080/api/queries \
  -H "Content-Type: application/json" \
  -d '{"query_id":"q1","janusql":"PREFIX ex: <http://example.org/> REGISTER RStream ex:o AS SELECT ?s ?p ?o FROM NAMED WINDOW ex:w ON STREAM ex:s [START 1704067200 END 1735689599] WHERE { WINDOW ex:w { ?s ?p ?o . } }"}'
```

### Start Replay
```bash
curl -X POST http://localhost:8080/api/replay/start \
  -H "Content-Type: application/json" \
  -d '{"input_file":"data/sensors.nq","broker_type":"mqtt","topics":["sensors"],"rate_of_publishing":1000,"loop_file":true,"mqtt_config":{"host":"localhost","port":1883,"client_id":"janus","keep_alive_secs":30}}'
```

## WebSocket (JavaScript)

```javascript
const ws = new WebSocket('ws://localhost:8080/api/queries/q1/results');
ws.onmessage = (e) => console.log(JSON.parse(e.data));
```

## Troubleshooting

```bash
# Check MQTT
docker ps | grep mosquitto

# Check server
curl http://localhost:8080/health

# View MQTT messages
docker exec -it janus-mosquitto mosquitto_sub -t "sensors" -v

# Restart MQTT
docker-compose restart mosquitto
```

## File Locations

```
janus/
├── examples/demo_dashboard.html  # Local demo client
├── docs/README_HTTP_API.md       # Current HTTP guide
├── docs/QUICKSTART_HTTP_API.md   # Short API quickstart
├── docs/SETUP_GUIDE.md           # Detailed setup
└── scripts/test_setup.sh         # Local setup helper
```

## Success Checklist

- [ ] MQTT running: `docker ps | grep mosquitto`
- [ ] Server running: `curl localhost:8080/health`
- [ ] Data exists: `ls data/sensors.nq`
- [ ] Query registers successfully
- [ ] Query starts successfully
- [ ] WebSocket receives results

---

**Quick Start:** `./scripts/test_setup.sh` then `cargo run --bin http_server`
