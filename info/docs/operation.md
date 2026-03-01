# Operation

This section is for operators running Synchronic Web infrastructure in real environments.
It covers deployment, service roles, network API behavior, routine operations, and protocol specifications.
Unless otherwise noted, examples target the `sync-services/compose/general` stack.

## Deployment

### Docker Compose

The compose deployment is the baseline operating mode for local, staging, and many production-like environments.
It provides a predictable process model so operators can troubleshoot behavior without guessing which component wiring changed.

Required/important environment variables:

- `SECRET` (required): interface authentication secret
- `PORT` (default `8192`): published HTTP port
- `PERIOD` (default `2`): stepping cadence parameter passed to journal
- `WINDOW` (default `1024`): retention window for unpinned history
- `LOCAL_LISP_PATH` (optional in test scripts): local override path for Lisp class files

Start:

```bash
SECRET=password PORT=8192 ./tests/up-compose.sh
```

Smoke validation:

```bash
./tests/smoke-compose.sh
```

Stop:

```bash
docker compose -f compose/general/docker-compose.yml down -v
```

## Services

The service split allows teams to scale or replace one layer at a time.
For example, interface routing can be adjusted independently from journal runtime updates.

### Interface

The interface service is the public nginx entry point for the deployment.
It serves runtime query routes and UI routes:

| Route | Purpose |
| --- | --- |
| `/interface` | Lisp request endpoint for direct evaluator and API calls. |
| `/interface/json` | JSON request endpoint for web and service integrations. |
| `/explorer/` | Browser UI for navigating and editing ledger and record paths. |
| `/workbench/` | Developer-oriented query workspace with API aids. |

### Journal

The journal service is the Rust runtime (`journal-sdk`), which loads the boot script, executes periodic step commands, and hosts the evaluator-backed interfaces consumed by the web layer.

### Explorer

Explorer is the operator-facing UI for browsing and editing ledger state, especially peer-visible paths and operational state transitions.

### Workbench

Workbench is the developer-facing query console and API reference surface used for payload authoring, validation, and ad hoc diagnostics.

## Network API

Implemented by `compose/general/interface.scm` on top of `control.scm` and `ledger.scm`.
Permission boundaries are central to safe operation, so operators should treat function families as security domains.

### Function Families

| Family | Description | Functions |
| --- | --- | --- |
| Public (no authentication) | Read-only methods intended for external visibility without secrets. | `size`, `synchronize`, `resolve`, `information` |
| Restricted (interface secret) | Stateful or sensitive operations that require interface authentication. | `get`, `set!`, `pin!`, `unpin!`, `general-peer!`, `peer!`, `peers`, `configuration`, `step-generate`, `step-chain!`, `step-peer!`, `*secret*` |
| Root/Admin control | Root control commands for runtime mutation and administrative lifecycle management. | `*eval*`, `*call*`, `*step*`, `*set-secret*`, `*set-step*`, `*set-query*` |

### Function Reference

Use this reference when building runbooks or on-call procedures.
The same function names appear in Explorer/Workbench, automated scripts, and incident diagnostics.

#### Public Functions

| Function | Purpose |
| --- | --- |
| `size` | Current permanent-chain size |
| `synchronize` | Serialize digest/proof data for peer sync |
| `resolve` | Resolve and serialize state at a path/index |
| `information` | Public node metadata (e.g., public key/window) |

#### Restricted Functions

| Function | Purpose |
| --- | --- |
| `get` | Read staged or historical data |
| `set!` | Stage a write |
| `pin!` | Persist proof/data in permanent chain |
| `unpin!` | Remove persisted pin |
| `general-peer!` | Register peer from URL helper |
| `peer!` | Register peer with explicit RPC handlers |
| `peers` | List configured peers |
| `configuration` | Return full public/private config |
| `step-generate` | Build ordered step actions |
| `step-chain!` | Commit staged state to chain |
| `step-peer!` | Sync state from a named peer |
| `*secret*` | Rotate interface secret |

#### Root/Admin Functions

| Function | Purpose |
| --- | --- |
| `*eval*` | Evaluate arbitrary Lisp in admin context |
| `*call*` | Invoke function with root object |
| `*step*` | Execute full step cycle |
| `*set-secret*` | Rotate root/admin secret |
| `*set-step*` | Replace step handler |
| `*set-query*` | Replace query handler |

### Control Plane API

`control.scm` provides authentication of privileged operations, enforces root mutation/update discipline, and defines hook points for query and step behavior.

#### Root Object Methods

These are the public root methods exposed by the control module's base record object.

| Method | Signature | Description |
| --- | --- | --- |
| `get` | `((root 'get) path)` | Read value at path; returns value, `(nothing)`, or directory metadata. |
| `set!` | `((root 'set!) path value)` | Write value at path; supports structured node/object values and deletion via `(nothing)`. |
| `copy!` | `((root 'copy!) source target)` | Copy value from `source` path to `target` path. |
| `equal?` | `((root 'equal?) source target)` | Exact structural equality check between two paths. |
| `equivalent?` | `((root 'equivalent?) source target)` | Digest-equivalence check between two paths. |

#### Root Control Commands

These are public control commands handled by the transition function in `control.scm`.

| Command | Signature | Description |
| --- | --- | --- |
| `*eval*` | `(*eval* <admin-secret> <expression>)` | Evaluate expression in admin context. |
| `*call*` | `(*call* <admin-secret> <function>)` | Invoke function with `root` object and persist resulting state. |
| `*step*` | `(*step* <admin-secret>)` | Execute configured step handler pipeline. |
| `*set-secret*` | `(*set-secret* <old> <new>)` | Rotate admin secret used by control plane. |
| `*set-step*` | `(*set-step* <admin-secret> <step-function>)` | Replace step handler logic. |
| `*set-query*` | `(*set-query* <admin-secret> <query-function>)` | Replace query handler logic. |

This API is intentionally powerful and low-level, so access should be tightly controlled and audited.

### General Interface API

`interface.scm` overlays the instantiated ledger object and dispatches `((function ...))` requests to public ledger methods, with permission checks on each call.
It also adds one integration helper (`general-peer!`) that composes the lower-level peer registration flow.

#### General API Methods

| Method | Permission | Description |
| --- | --- | --- |
| `size` | Public | Return permanent chain size. |
| `synchronize` | Public | Return serialized sync proof at index. |
| `resolve` | Public | Return serialized resolved path proof. |
| `information` | Public | Return public node configuration. |
| `get` | Restricted | Read staged or historical content. |
| `set!` | Restricted | Stage write to local state path. |
| `pin!` | Restricted | Persist selected path/proof in permanent chain. |
| `unpin!` | Restricted | Remove previously pinned path/proof. |
| `general-peer!` | Restricted | Add peer from URL and auto-wire `information/synchronize/resolve` handlers. |
| `peer!` | Restricted | Add peer with explicit handler expressions. |
| `peers` | Restricted | List configured peer names. |
| `configuration` | Restricted | Return full configuration (including private fields). |
| `step-generate` | Restricted | Generate ordered step actions. |
| `step-chain!` | Restricted | Commit staged state and advance chain. |
| `step-peer!` | Restricted | Synchronize a named peer into local staged state. |
| `*secret*` | Restricted | Rotate interface secret used by general API auth checks. |

Root commands (`*eval*`, `*call*`, `*step*`, `*set-secret*`, `*set-step*`, `*set-query*`) are also callable, but remain part of the control-plane surface and should be treated as admin-level operations.
This keeps externally visible behavior stable while allowing implementation details inside ledger classes to evolve.

## Operations

This section provides common operational procedures.
Each example includes both Lisp and JSON forms.
These are direct calls that can be run manually, but they also serve as templates for operational automation.

### Secret Rotation

#### Rotate interface secret

=== "Lisp"
    ```scheme
    ((function *secret*)
     (arguments ("old-password" "new-password"))
     (authentication "old-password"))
    ```

=== "JSON"
    ```json
    {
      "function": "*secret*",
      "arguments": [
        {"*type/string*": "old-password"},
        {"*type/string*": "new-password"}
      ],
      "authentication": {"*type/string*": "old-password"}
    }
    ```

#### Rotate root/admin secret

=== "Lisp"
    ```scheme
    (*set-secret* "old-admin" "new-admin")
    ```

=== "JSON"
    ```json
    [
      "*set-secret*",
      {"*type/string*": "old-admin"},
      {"*type/string*": "new-admin"}
    ]
    ```

### Updating Runtime Logic

Use root-level calls for controlled live updates.
Typical update targets are: control hooks, standard, configuration, tree, chain, and ledger.
Because these changes alter live semantics, apply them in lower environments first and validate with smoke checks before promotion.

#### Update control hooks (`*set-query*`, `*set-step*`)

=== "Lisp"
    ```scheme
    (*set-query* "admin-password"
      (lambda (root query)
        (if (equal? query '(*ping*)) 'pong (error 'query-error "No handler"))))
    ```

=== "JSON"
    ```json
    [
      "*set-query*",
      {"*type/string*": "admin-password"},
      [
        "lambda",
        ["root", "query"],
        [
          "if",
          ["equal?", "query", {"*type/quoted*": ["*ping*"]}],
          {"*type/quoted*": "pong"},
          ["error", {"*type/quoted*": "query-error"}, {"*type/string*": "No handler"}]
        ]
      ]
    ]
    ```

For step hook updates, use the same pattern with `*set-step*`.

#### Update standard class

=== "Lisp"
    ```scheme
    (*call* "admin-password"
      (lambda (root)
        (define standard-cls '(define-class (standard) ...))
        ((root 'set!) '(control class standard) standard-cls)
        ((root 'set!) '(control object standard)
          (((eval (caddr standard-cls)) #f standard-cls)))))
    ```

=== "JSON"
    ```json
    [
      "*call*",
      {"*type/string*": "admin-password"},
      [
        "lambda",
        ["root"],
        [
          "begin",
          ["define", "standard-cls", {"*type/quoted*": ["define-class", ["standard"], "..."]}],
          [["root", {"*type/quoted*": "set!"}], {"*type/quoted*": ["control", "class", "standard"]}, "standard-cls"]
        ]
      ]
    ]
    ```

#### Update configuration state

=== "Lisp"
    ```scheme
    ((function configuration)
     (authentication "password"))
    ```

=== "JSON"
    ```json
    {
      "function": "configuration",
      "authentication": {"*type/string*": "password"}
    }
    ```

For in-place configuration mutation, use root-level `*call*` and update `(control object ledger)` with the mutated object.

#### Update tree/chain/ledger classes

Operational pattern:

1. Replace class definition under `(control class <name>)`.
2. Rebuild affected object with `standard.make`.
3. Reinsert updated object under `(control object ledger)` or related path.
4. Validate with smoke tests.

=== "Lisp"
    ```scheme
    (*call* "admin-password"
      (lambda (root)
        (define ledger-cls '(define-class (ledger) ...))
        ((root 'set!) '(control class ledger) ledger-cls)
        ...))
    ```

=== "JSON"
    ```json
    [
      "*call*",
      {"*type/string*": "admin-password"},
      [
        "lambda",
        ["root"],
        ["begin", ["define", "ledger-cls", {"*type/quoted*": ["define-class", ["ledger"], "..."]}], "..."]
      ]
    ]
    ```
