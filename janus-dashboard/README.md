# Janus Dashboard (Local Demo)

This directory contains a lightweight local dashboard used for quick backend demos from within the `janus` repository.

It is not the primary dashboard codebase.

The maintained dashboard lives in a separate repository:

- `https://github.com/SolidLabResearch/janus-dashboard`

## What This Folder Is For

- quick local testing against `http_server`
- validating the WebSocket result stream manually
- lightweight backend demo flows during engine development

## What This Folder Is Not For

- the main frontend product surface
- long-term frontend feature development
- frontend CI ownership for the Janus project as a whole

## Local Use

Install dependencies:

```bash
npm install
```

Run checks:

```bash
npm run check
```

Start the dev server:

```bash
npm run dev
```

The local app expects the Janus backend to be running on `http://localhost:8080`.
