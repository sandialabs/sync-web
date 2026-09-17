# Synchronic Web Explorer

React UI for browsing and editing synchronic web journals through the gateway API.

## Current Model

The Explorer has four modes:

- `Ledger`
  - committed browsing at `Self` or one explicitly selected terminal responder
  - one left navigation tree rooted at `State` and `Bridges`
  - retained bridge journal names expand to exact materialized indexes and authorized nested `State`
  - document view toggles between content and proof
- `Stage`
  - staged documents and directories at `Self` or the selected working journal
  - remote reads/writes use federated blank read-only `use!`/`put!`; mutation controls are optimistic and the terminal journal remains authoritative
  - the `(*state*)` namespace root is browsing-only; selected descendants expose mutation controls even when a remote policy may deny the request
  - denied mutations show operation-local errors without changing the tree selection, and denied document saves preserve editor content
  - tree-driven selection; document editing is read-only until `Edit`
- `Access`
  - Self-local management of path-scoped blank read-only `use!`, `put!`, `run!`, and `retrieve` rules for authenticated local and remote principals
  - disabled while a remote working route is selected
- `Admin`
  - visible only to local interface admins and enabled only at `Self`
  - bridge registration and local bridge endpoint copying
  - public window-size editing
  - interface admin-list management

There is no dedicated history pane. The working route selects one responder; Ledger history and retained bridge indexes are navigated in the left `State`/`Bridges` tree. The separate snapshot strip remains only for Self-local Ledger browsing and is hidden for an explicitly routed responder.

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

- Without a valid explicit deep link, Explorer opens Stage at the selected terminal journal's `(*state*)` namespace root. Explicit supported Stage, Ledger, Access, and Admin links remain authoritative. Stage and Ledger roots retain a permanent `*state*` row at the top of the tree, which returns to that exact Stage route or Ledger route/snapshot root. The signed-in user's folder remains its first bold child; it is never relabeled.
- Working-route breadcrumbs are clickable. Selecting `Self`, an intermediate journal, or the current terminal verifies that route prefix and resets both mode selections to its `*state*` root while preserving prefix Ledger snapshots. Adding or removing a hop follows the same rule rather than carrying an owner-relative suffix between journals.
- Switching between `Stage` and `Ledger` preserves the working route and each tab's selection. Leaving and re-entering Ledger resolves fresh exact hop indexes; traversal within one Ledger visit keeps its displayed indexes fixed. `Access` and `Admin` remain Self-local controls.
- Explicit deep links remain exact, while synchronization and snapshot changes preserve an accessible path and otherwise fall back to the current terminal root.
- Stage browsing and editing send only blank read-only `use!` and `put!` across the working route; delete and ordinary document edits are expressed through `put!`. Federated `run!` is a separate application operation whose independent local/remote grants are managed in Access; Explorer does not execute staged programs. The Stage namespace root is browsing-only, and mutation controls are also hidden for the conventionally public `data/public` subtree on a remote route.
- Tree rows expose rename and delete.
- Explorer encodes every user-created path name and bridge alias as one reversible reader-safe Scheme symbol. ASCII letters, digits after the first byte, `_`, and `-` remain literal; every other UTF-8 byte uses uppercase `%HH`, including literal `%` as `%25`. Explorer decodes names only for display and retains exact encoded segments for navigation, refresh, authorization, pinning, and routes. Empty or malformed-Unicode names fail visibly.
- Selecting a directory exposes `+ Put`, `+ Directory`, and `Upload Document`. Put is create-only and atomically requires target absence. Storage and input are independent: Bytes accepts one exact typed JSON byte vector or Scheme `#u(...)` without conversion; Expression accepts one complete JSON or Scheme value; Object requires one complete Scheme `define-class` value and stores an uninitialized shell.
- Selecting a document exposes `Edit` / `Save` and `Download`. The Object pane appears only when an ordinary read-only `*api*` probe succeeds through the existing Journal JSON/Standard object boundary; inert Bytes and Expression values reject that structured probe and never render Object controls. Explorer then displays the original blank-call metadata and a separate ordinary Scheme `*api*` result as raw text without parsing, interpreting, or deriving controls from either. Stage `use!` starts read-only; turning it off permits persistence only after successful execution.

### Ledger

- The working route above the tree selects exactly one responder; Explorer never searches or ranks alternate providers.
- One route bar shows `Self` and every selected bridge. Each hop resolves `latest` to an exact displayed index when entered; right-hand traversal, selection, and shortening preserve every earlier index. Only explicit Refresh or leaving and re-entering Ledger may refresh resolved indexes.
- The left tree always has `State` and `Bridges` roots. `Bridges` lists responder-permanent, available inventories; journal rows expand to ascending exact integer directory nodes. Each index independently admits `State` and recursively retained `Bridges`, and an index with no admitted children remains navigable as `No accessible contents`.
- Retained reads send responder-local committed paths through the selected `$federation.route`. Showing an authenticated inventory index grants no child access. Temp-only, unavailable, or unauthorized children stay out of navigation, and failures never contact the attributed source.
- Deep links preserve the terminal route separately from the recursively resolved retained path.
- Selecting a document exposes `Proof` / `Content` and `Pin` / `Unpin`. Historical Object execution uses `retrieve`; its visible Read only control is checked and disabled because no Stage successor can persist.
- Remote pin/unpin retains proof at `Self`; it captures one explicit origin snapshot for retrieve and local pin, and does not mutate terminal retention. Explorer changes the displayed state only after exact local readback, verifies every selected bridge index in the resulting sparse inventories, and requires matching terminal-provider content. Indeterminate readback never leaves an optimistic pin claim.

### Access

- Rules are owned by a local `(*state* <user>)` namespace. Administrators explicitly load only a username shorthand or exact `*state* USER`; stale namespace responses cannot replace the current target.
- Owner-relative paths keep simple whitespace-separated segments and also accept quoted human segments such as `data "private documents"`. Backslash escapes the next literal quote, backslash, or whitespace character. Human segments are encoded exactly once on add and decoded for display; for example, literal `a%20b` is stored as `a%2520b`. The untouched stored path remains the exact delete identity.
- Principal choices are exactly `Local user`, `Remote user`, and `Public`. Journal location is shown and required only for Remote user. Local user emits `(*state* USER)`, Public emits `(*public*)`, and Remote user emits the exact route followed by `*state* USER`.
- Remote principals are exact bridge paths with one or more route segments followed by `*state* USER`. Explorer automatically adds the fixed terminal authentication range `key-index (-32 -1)` without exposing controls for it. Partial or malformed principals remain visible for correction and fail clearly on submit. Exact local `(*state* USER)` and `(*public*)` principals omit `key-index`.
- A blank or whitespace-only path is the selected user's recursive home directory under `(*state* USER)`, stored as `path: []` and displayed as `(home)`. It does not grant Journal Root authority. Success is shown only after the exact rule appears in a refreshed authorization read; false, duplicate, rejected, or absent postconditions preserve the form and report a non-success result.
- Permission controls and summaries use the order `put`, `use`, `retrieve`, `run`; Scheme punctuation remains confined to internal and wire names. Selecting `use` reveals its `read-only` qualifier in the same pill, default false. Selecting `retrieve` reveals compact `index start` and `index end` inputs in the same pill, default `0` and `-1`.
- The complete raw stored rule—including its hidden fixed remote `key-index` and internal operation field names—is retained as the exact deletion identity. Access exposes independent `run` grants for authenticated local and remote users; ownership and read-only `use` do not imply execution, while root/configured local Interface administrators retain default access.
- Return the working route to `Self` before managing rules; disabled remote-route tabs are labeled `Access · Self` and `Admin · Self`.

### Admin

- The tab appears only after the current session succeeds against the admin-gated `admins` endpoint and is disabled on remote working routes.
- The Bridges section composes Ledger public settings with the explicit private Federation bridge view. Its read-only identity block shows the advertised/default journal name beside the local endpoint that other journals should use: `http(s)://<host>/api/v1/journal/interface`.
- Existing bridge cards report their configured endpoint, peer-side name, initiation direction, and available progress metadata. Missing values are labeled `Not reported` or `Direction not reported`; they never imply a false role.
- Bridge registration stores a concrete peer interface URL, not a generic API base URL. Incoming preapproval uses the 64-hex-character SHA-256 hash of the peer's raw signing public-key bytes; the full signing key remains part of ordinary signature verification. Bridge deletion confirms that bridge configuration and alias-scoped authorization state cascade; incoming preapproval removal also requires confirmation.
- Window size decreases require confirmation showing the authoritative old/new values and warn that pruned unpinned history is not resurrected by later widening. Unknown current state refuses mutation; equal/increased windows do not confirm.
- Admin user changes replace the interface admin list through the gateway; only local `(*state* <name>)` principals are valid administrators.
- Admin mutations are serialized, disable every mutation control while pending, preserve failed form values, and clear forms only after a successful mutation and authoritative reload.

### Basic click-through QA

Before accepting an Explorer image:

1. As an ordinary user at Self, open the bridge picker, choose a peer, reopen the picker at that terminal peer, and extend the route using public bridge metadata without an authorization error.
2. In Ledger, expand a retained bridge and verify inventory indexes remain directory nodes even when they have no accessible children; expand `State` and nested `Bridges` independently.
3. In Access, verify `put`, `use`, `retrieve`, `run` order and progressive `read-only` / index controls at desktop, compact, and mobile widths.
4. In Admin, verify the read-only journal identity block and truthful endpoint, peer-side name, direction, and progress fields at desktop and mobile widths.
5. Confirm the browser console and failed-request capture contain no unexpected authorization, routing, or rendering errors.

The deployed bridge regression makes step 1 executable against a prepared three-Journal topology. The ordinary user's first and second peers must expose public bridge metadata, grant the required State roots, and have a provider index different from the requester's index. Use the topology's `localhost` login origin so Kratos returns the browser session cookie to Explorer. The gate requires the ordinary user's expected 400 denial from the exact `/general/admins` capability probe, while failing closed if the topology is not divergent, bridge discovery omits `read-only?`, requester admission is routed to the provider, any other `/general/*` request fails, or the second hop is not reached.

```bash
EXPLORER_ORIGIN=http://localhost:8080 \
EXPLORER_PROVIDER_ORIGIN=http://localhost:8081 \
EXPLORER_USERNAME=alice EXPLORER_PASSWORD=password \
EXPLORER_FIRST_PEER=journal-b EXPLORER_SECOND_PEER=journal-c \
PLAYWRIGHT_NODE_MODULES="$HOME/.cache/sync-web/playwright/node_modules" \
npm run test:deployed-bridge-journey
```

The deployed exploratory soak exercises the event, error, edit, mounted-logo, and transient-picker regressions against the same prepared topology. It enforces at least 600 seconds, preserves unsaved text across a genuine change hint, verifies the saved value after reload, and fails on recursive successful or failing reads, missing mutation events, unexpected console/network errors, or a non-rendering `/explorer/logo.png` at desktop and mobile widths.

```bash
EXPLORER_ORIGIN=http://localhost:8080 \
EXPLORER_USERNAME=alice EXPLORER_PASSWORD=password \
EXPLORER_FIRST_PEER=journal-b EXPLORER_SOAK_SECONDS=600 \
PLAYWRIGHT_NODE_MODULES="$HOME/.cache/sync-web/playwright/node_modules" \
npm run test:deployed-exploratory-soak
```

Both deployed runners use Podman or Docker and pin the browser image to Playwright 1.55.0. `PLAYWRIGHT_NODE_MODULES` must contain the matching `playwright` package; keeping it outside this production-only frontend avoids adding browser tooling to the Explorer image.

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
