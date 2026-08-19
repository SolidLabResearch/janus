# Janus Live Streaming Guide

Live Janus queries use an MQTT-backed stream path. The local Compose setup
provides Eclipse Mosquitto on `localhost:1883`.

## Start the local services

```bash
docker-compose up -d mosquitto
cargo run --bin http_server -- --host 127.0.0.1 --port 8080 --storage-dir ./data/storage
```

Check the broker separately if a live query does not receive results:

```bash
docker exec -it janus-mosquitto mosquitto_sub -t sensors -v
```

## Replay RDF events

The CLI reads N-Triples or N-Quads, writes events to storage, and optionally
publishes them to MQTT:

```bash
cargo run --bin stream_bus_cli -- \
  --input data/sensors.nq \
  --broker mqtt \
  --topics sensors \
  --rate 64 \
  --mqtt-host localhost \
  --mqtt-port 1883
```

Use `--broker none` to write to storage without publishing. Use `--rate 0` for
unlimited publication and `--loop-file` to repeat the source file.

## Register a live query

Use an `ON STREAM` declaration with a positive range and step:

```sparql
PREFIX ex: <http://example.org/>
REGISTER RStream ex:output AS
SELECT ?sensor ?value
FROM NAMED WINDOW ex:live ON STREAM ex:stream [RANGE 60000 STEP 30000]
WHERE { WINDOW ex:live { ?sensor ex:hasValue ?value . } }
```

Register and start it through the [HTTP API](./HTTP_API_CURRENT.md), then
subscribe to `WS /api/queries/:id/results` before replaying data.

## Troubleshooting

- Check `/health` and `/ops/status` before diagnosing the query.
- Verify the broker host, port, and topic used by replay match the live setup.
- Use the WebSocket result endpoint, not an HTTP poll, to receive running-query
  results.
- Stop the query and replay explicitly when repeating an experiment.
