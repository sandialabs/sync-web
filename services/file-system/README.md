# File System Service

The file-system service projects sync-web journal paths through WebDAV. It replaces the
old SMB implementation; no SMB compatibility layer is preserved.

## Status

The Go service implements the core WebDAV surface against the gateway-backed journal:
`PROPFIND`, `GET`, `HEAD`, `PUT`, `DELETE`, `MKCOL`, `COPY`, `MOVE`, and write-only
`/control/pin` directives. Writes stage data only; this service never steps the journal.

## Routes

- `GET /health` — readiness/health check.
- `/webdav/stage/<path...>` — mutable staged state.
- `/webdav/ledger/state/<path...>` — latest committed state shorthand.
- `/webdav/ledger/<digit>/<digit>/.../` — read-only collection for a known explicit nonnegative index.
- `/webdav/ledger/minus/<digit>/<digit>/.../` — read-only collection for a known explicit negative index.
- `/webdav/ledger/<digit>/<digit>/.../state/<path...>` — committed state at an explicit nonnegative index.
- `/webdav/ledger/minus/<digit>/<digit>/.../state/<path...>` — committed state at an explicit negative index.
- `/webdav/ledger/bridge/<name>/state/<path...>` — latest committed bridge shorthand.
- `/webdav/ledger/<local-index-digits>/bridge/<name>/<target-index-digits>/` — read-only collection for a known explicit bridge target index.
- `/webdav/ledger/<local-index-digits>/bridge/<name>/<target-index-digits>/state/<path...>` — explicit bridge traversal. Use `minus/` before digit segments for negative local or target indexes.
- `/webdav/control/pin` — write-only pin/unpin directive sink.

## Configuration

- `SYNC_FS_ADDRESS` — listen address, default `:8080`.
- `SYNC_FS_GATEWAY_BASE_URL` — gateway API base URL, default `http://gateway/api/v1`.
- `SYNC_FS_MAX_OBJECT_BYTES` — maximum object size, default `1048576`.

Authentication is delegated to the existing Kratos-backed stack. WebDAV clients should use
Basic Auth with a Sync Web API token as the password. The username can be the sync-web
username (for example, `admin`), but WebDAV does not accept the account password because
it cannot perform the browser login flow.

Path names are translated at the WebDAV boundary. Names that are not safe unescaped
R7RS-style identifier symbols are UTF-8 percent-escaped before gateway calls and decoded
again for WebDAV directory listings. The ledger stores ordinary symbol atoms such as
`New%20folder`; it does not interpret percent escapes.

## Development

```bash
go test ./...
go run ./cmd/file-system
```

Container test target:

```bash
${CONTAINER_RUNTIME:-docker} build --target test services/file-system
```
