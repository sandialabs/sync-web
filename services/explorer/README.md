# Synchronic Web Explorer

React UI for browsing and editing synchronic web journals through the gateway API.

## Current Model

The Explorer has four modes:

- `Ledger`
  - committed route-based browsing
  - route strip across the top
  - document view toggles between content and proof
- `Stage`
  - staged documents and directories at `Self` or the selected working journal
  - remote reads/writes use federated `get`/`set!`; mutation controls are optimistic and the terminal journal remains authoritative
  - the `(*state*)` namespace root is browsing-only; selected descendants expose mutation controls even when a remote policy may deny the request
  - denied mutations show operation-local errors without changing the tree selection, and denied document saves preserve editor content
  - tree-driven selection; document editing is read-only until `Edit`
- `Access`
  - Self-local path-scoped `get`, `set!`, and `resolve` rules
  - disabled while a remote working route is selected
- `Admin`
  - visible only to local interface admins and enabled only at `Self`
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

- Without an explicit deep link, Stage and Ledger start at the selected terminal journal's `(*state*)` namespace root. A permanent `*state*` row at the top of the tree returns to that exact Stage route or Ledger route/snapshot root. The signed-in user's folder remains its first bold child; it is never relabeled.
- Working-route breadcrumbs are clickable. Selecting `Self`, an intermediate journal, or the current terminal verifies that route prefix and resets both mode selections to its `*state*` root while preserving prefix Ledger snapshots. Adding or removing a hop follows the same rule rather than carrying an owner-relative suffix between journals.
- Every click on the `Stage` or `Ledger` tab, including the active tab, returns the global working route to `Self`, resets both mode selections to Self `*state*`, and selects that mode. `Access` and `Admin` remain Self-local controls.
- Explicit deep links remain exact, while synchronization and snapshot changes preserve an accessible path and otherwise fall back to the current terminal root.
- Only `get` and `set!` cross the working route; delete and ordinary document edits are expressed through `set!`. The Stage namespace root is browsing-only, and mutation controls are also hidden for the conventionally public `data/public` subtree on a remote route.
- Tree rows expose rename and delete.
- Selecting a directory exposes:
  - `+ Document`
  - `+ Directory`
  - `Upload Document`
- Selecting a document exposes:
  - `Edit` / `Save`
  - `Download`

### Ledger

- The route strip spans the app above the tree and content pane.
- The leftmost pill synchronizes the latest committed root and shows the current root index.
- The first hop is always the local/root journal.
- Each hop accepts:
  - `latest`
  - a negative integer snapshot index
- Extending the route opens an inline peer chooser and refreshes a `latest` origin snapshot before selecting the new committed route.
- Selecting a document exposes:
  - `Proof` / `Content`
  - `Pin` / `Unpin`
- Remote pin/unpin retains proof at `Self`; it captures one explicit origin snapshot for resolve and local pin, and does not mutate terminal retention.

### Access

- Rules are owned by a local `(*state* <user>)` namespace. Administrators explicitly load only a username shorthand or exact `*state* USER`; stale namespace responses cannot replace the current target.
- Owner-relative paths keep simple whitespace-separated segments and also accept quoted human segments such as `data "private documents"`. Backslash escapes the next literal quote, backslash, or whitespace character. Human segments are encoded exactly once on add and decoded for display; for example, literal `a%20b` is stored as `a%2520b`. The untouched stored path remains the exact delete identity.
- Remote principals are exact bridge paths with one or more route segments followed by `*state* USER`. Explorer automatically adds the fixed terminal authentication range `key-index (-32 -1)` without exposing controls for it. Partial or malformed principals remain visible for correction and fail clearly on submit. Exact local `(*state* USER)` and `(*public*)` principals omit `key-index`.
- Resolve has a separate displayed `Document history window`, initially `0 … -1`, that constrains document-history indexes. The complete raw stored rule—including its hidden fixed remote `key-index`—is retained as the exact deletion identity.
- Application-rule permissions are `get`, `set!`, and `resolve`. `call!` is not shown or grantable in Access: it is limited to root/configured local Interface administrators, independent of namespace ownership.
- Return the working route to `Self` before managing rules; disabled remote-route tabs are labeled `Access · Self` and `Admin · Self`.

### Admin

- The tab appears only after the current session succeeds against the admin-gated `admins` endpoint and is disabled on remote working routes.
- The Bridges section shows the local endpoint that other journals should use when registering this node:
  - `http(s)://<host>/api/v1/journal/interface`
- Bridge registration stores a concrete peer interface URL, not a generic API base URL. Bridge deletion confirms that bridge configuration and alias-scoped authorization state cascade; incoming preapproval removal also requires confirmation.
- Window size decreases require confirmation showing the authoritative old/new values and warn that pruned unpinned history is not resurrected by later widening. Unknown current state refuses mutation; equal/increased windows do not confirm.
- Admin user changes replace the interface admin list through the gateway; only local `(*state* <name>)` principals are valid administrators.
- Admin mutations are serialized, disable every mutation control while pending, preserve failed form values, and clear forms only after a successful mutation and authoritative reload.

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
