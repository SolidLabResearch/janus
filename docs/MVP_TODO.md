# Janus MVP TODO

This file is retained as a planning artifact.

It no longer describes the current backend state accurately as a whole.

## What Is Already Present

The repository already includes:

- JanusQL parsing
- historical query execution
- live query execution
- HTTP/WebSocket endpoints
- replay control endpoints
- integration tests for the Janus API and HTTP server

## What Still Needs Follow-Up

The remaining work is no longer “build the MVP from scratch”. The current gaps are narrower:

### Documentation coherence

- keep top-level docs aligned with the actual binaries and API
- clearly separate current guides from historical design notes
- document the backend/dashboard repo split

### Runtime lifecycle hardening

- increment query execution counts when queries start
- tighten shutdown and worker cleanup behavior
- manage spawned MQTT subscriber handles consistently
- make query status transitions more explicit

### Repo boundary cleanup

- treat the local dashboard as a demo client
- keep product dashboard work in the separate `janus-dashboard` repository

## Historical Note

Earlier versions of this file described `JanusApi::start_query()` and the HTTP path as missing. That is no longer true in the current codebase.
