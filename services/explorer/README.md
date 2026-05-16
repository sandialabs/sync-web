# Synchronic Web Explorer

React UI for browsing and editing synchronic web journals through the gateway API.

## Current Model

The explorer now has three modes:

- `Ledger`
  - committed route-based browsing
  - route strip across the top
  - file view toggles between content and proof
- `Stage`
  - local staged files and directories
  - tree-driven selection
  - file editing is read-only until `Edit`
- `Admin`
  - visible only to interface admins
  - bridge registration and local bridge endpoint copying
  - public window-size editing
  - interface admin-list management

There is no dedicated history pane in the current UI. Ledger history is expressed through the route strip and snapshot controls.

## Development

Prerequisites:

- Node.js 18+
- npm

Run the dev server:

```bash
REACT_APP_SYNC_EXPLORER_ENDPOINT=http://127.0.0.1:8192/api/v1 \
REACT_APP_SYNC_EXPLORER_PASSWORD=password \
npm start
```

The app is served from `http://localhost:3000/explorer`.

## Configuration

Development-time defaults are read from:

- `REACT_APP_SYNC_EXPLORER_ENDPOINT`
- `REACT_APP_SYNC_EXPLORER_PASSWORD`

Container/runtime defaults are read from:

- `SYNC_EXPLORER_ENDPOINT`
- `SYNC_EXPLORER_PASSWORD`

## Usage Notes

### Stage

- The left tree shows local staged content.
- Tree rows expose rename and delete.
- Selecting a directory exposes:
  - `+ Document`
  - `+ Directory`
  - `Upload File`
- Selecting a file exposes:
  - `Edit` / `Save`
  - `Download`

### Ledger

- The route strip spans the app above the tree and content pane.
- The leftmost pill synchronizes the latest committed root and shows the current root index.
- The first hop is always the local/root journal.
- Each hop accepts:
  - `latest`
  - a negative integer snapshot index
- Extending the route opens an inline peer chooser.
- Selecting a file exposes:
  - `Proof` / `Content`
  - `Pin` / `Unpin`

### Admin

- The tab appears only after the current session succeeds against the admin-gated `admins` endpoint.
- The Bridges section shows the local endpoint that other journals should use when registering this node:
  - `http(s)://<host>/api/v1/journal/interface`
- Bridge registration stores a concrete peer interface URL, not a generic API base URL.
- Window size changes call the interface-level window operation and require confirmation when decreasing the value.
- Admin user changes replace the interface admin list through the gateway.

## Docker

Build:

```bash
docker build -t explorer .
```

Run:

```bash
docker run -p 8080:80 \
  -e SYNC_EXPLORER_ENDPOINT=http://127.0.0.1:8192/api/v1 \
  -e SYNC_EXPLORER_PASSWORD=password \
  explorer
```
