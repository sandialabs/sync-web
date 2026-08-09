# Synchronic Web Workbench

A developer interface for querying synchronic web journals.

## Overview

The Synchronic Web Workbench provides a structured raw-Scheme interface to interact programmatically with synchronic web journals. It allows developers to write queries, view outputs, and explore the operations available through the configured journal interface endpoint.

## Development

### Prerequisites

- Node.js 18+
- npm

### Setup

    npm install

### Run locally

    npm start

The application will be available at http://localhost:3000

### Environment Variables

- REACT_APP_SYNC_WORKBENCH_ENDPOINT: Journal endpoint URL (default: http://localhost:4096/interface)

### Testing

    npm test

### Linting

    npm run lint

## Docker

### Build

    docker build -t synchronic-workbench .

### Run

    docker run -p 80:80 -e SYNC_WORKBENCH_ENDPOINT=http://your-journal:4096/interface synchronic-workbench

## Architecture

The application is a single-page React app with four main panes:

- **Left Pane**: verified API reference, functions, examples, and help documentation
- **Top Pane**: Query editor with multiple tabs
- **Bottom Pane**: Output viewer showing query, result, request, and response
- **Right Pane**: Query history

## Visual Design

The application uses a developer-focused design with a monospace font and IDE-like appearance. It supports both light and dark themes.

## API Catalog

`public/help-api.json` is audited against the active Interface query dispatcher in `records/lisp/interface.scm` and the six exposed root forms in `records/lisp/root.scm`. It is metadata and examples only; Workbench remains a raw Scheme client and does not alter endpoint authorization.

Permission filters describe the minimum caller role. Every user/admin template includes an explicit `authentication.identity`; omitting identity means the root journal caller and would not demonstrate that labeled tier.

Permission filters are:

- `any` / **Anon** — no caller identity required.
- `user` — authenticated operations available under ownership or explicit authorization.
- `admin` — configured local Interface-administrator operations. `call!`, bridge/config administration, admin/window controls, and interface-secret rotation are in this class.
- `root` — the separate six-operation root plane.

The admin-only `*secret*` operation rotates the journal-wide Interface authentication secret and corresponding Interface public signing key; it is not a per-user password change.

Authorization examples keep terminal authentication `key-index (-32 -1)` separate from Resolve document history `(0 -1)` and use the identical complete rule for `authorize!` and `deauthorize!`. Exact local/public rules omit `key-index`. Committed and federated paths are flat, for example `(-1 peer-a -1 *state* key)`.

Peer-internal `route` and `synchronize!` are intentionally not advertised as ordinary Workbench operations.

### Color Palette

- Blue: #00add0
- Medium Blue: #0076a9
- Dark Blue: #002b4c
- Supporting colors for syntax highlighting and status indicators
