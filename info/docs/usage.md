# Usage

This section covers day-to-day interaction patterns for users who need to read, write, pin, and synchronize state.
It focuses on the currently deployed `general` interface shape used by `sync-services`.

## Graphical Interface

The `general` compose network exposes two primary web clients:

Explorer at `/explorer/` is the state-navigation surface used for inspecting and updating record or ledger paths.
Workbench at `/workbench/` is the API-oriented workspace used to compose, run, and debug calls.

Core operations in both interfaces include reading data with `get`, staging writes with `set!`, pinning historical paths with `pin!`, and adding peers with `general-peer!`.

For most users, this is the fastest way to become productive because it exposes common operations without requiring immediate familiarity with the full request schema.
As confidence grows, the same operations can be moved into scripts and applications through the programmatic API.

## Programmatic API

Programmatic access is typically where teams standardize integrations across multiple services.
The sections below focus on stable request patterns that are easy to lint, test, and automate.

### Endpoint Catalog

Use this catalog as a quick orientation map.
The two execution endpoints (`/interface`, `/interface/json`) are for runtime calls, while the conversion endpoints are for debugging and payload authoring.

| Endpoint | Description |
| --- | --- |
| `POST /interface` | Executes Lisp requests directly against the interface runtime. |
| `POST /interface/json` | Executes JSON requests using the object-shaped API envelope. |
| `POST /interface/lisp-to-json` | Converts Lisp expressions into JSON payload representations. |
| `POST /interface/json-to-lisp` | Converts JSON payloads back into Lisp forms for debugging and validation. |

### Request Envelope

Most JSON integrations can standardize on this envelope and only vary `function` and `arguments`.
That keeps client implementations simple and makes troubleshooting easier across services.

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `function` | symbol/string | Yes | API function name |
| `arguments` | list/array | Usually | Positional function arguments |
| `authentication` | string / `*type/string*` | Restricted calls | Interface secret |

!!! note

    In Lisp form, API queries are association lists. In JSON form, they are objects with equivalent fields.

### Operation Examples

These examples are intentionally minimal and map directly to frequently used workflows.
You can copy them into Workbench first, then move them into automation once validated.

#### Read (`get`)

=== "Lisp"
    ```scheme
    ((function get)
     (arguments (((*state* docs article hash)) #t))
     (authentication "password"))
    ```

=== "JSON"
    ```json
    {
      "function": "get",
      "arguments": [
        [
          ["*state*", "docs", "article", "hash"],
          true
        ]
      ],
      "authentication": {"*type/string*": "password"}
    }
    ```

??? info "When to use `details? = true`"

    Use `#t` / `true` when you need proof and pin metadata, not just content.

#### Write (`set!`)

=== "Lisp"
    ```scheme
    ((function set!)
     (arguments (((*state* docs article hash)) "0xabc123"))
     (authentication "password"))
    ```

=== "JSON"
    ```json
    {
      "function": "set!",
      "arguments": [
        [
          ["*state*", "docs", "article", "hash"],
          {"*type/string*": "0xabc123"}
        ]
      ],
      "authentication": {"*type/string*": "password"}
    }
    ```

#### Pin (`pin!`)

=== "Lisp"
    ```scheme
    ((function pin!)
     (arguments ((-1 (*state* docs article hash))))
     (authentication "password"))
    ```

=== "JSON"
    ```json
    {
      "function": "pin!",
      "arguments": [[-1, ["*state*", "docs", "article", "hash"]]],
      "authentication": {"*type/string*": "password"}
    }
    ```

#### Peer (`general-peer!`)

=== "Lisp"
    ```scheme
    ((function general-peer!)
     (arguments (journal_b "http://journal-b.example.org/interface"))
     (authentication "password"))
    ```

=== "JSON"
    ```json
    {
      "function": "general-peer!",
      "arguments": [
        "journal_b",
        {"*type/string*": "http://journal-b.example.org/interface"}
      ],
      "authentication": {"*type/string*": "password"}
    }
    ```

## JSON/Lisp Conversion

The converter treats JSON and Lisp as structurally equivalent where possible.

### Core Mapping

This mapping is the key mental model for moving between web client payloads and Lisp-native request forms.
Once this is clear, advanced payloads become much easier to reason about.

| Lisp | JSON |
| --- | --- |
| symbol | string |
| list | array |
| association list `((k v) ...)` | object `{ "k": v, ... }` |
| object-shaped assoc list | object |

### Special Types

Special type wrappers are only needed where plain JSON cannot preserve Lisp/runtime semantics.
If a payload seems unexpectedly interpreted, check whether a special type marker is required.

| JSON marker | Lisp value |
| --- | --- |
| `{"*type/string*": "text"}` | string |
| `{"*type/quoted*": ...}` | `(quote ...)` |
| `{"*type/byte-vector*": "deadbeef"}` | byte-vector |
| `{"*type/vector*": [...]}` | vector |
| `{"*type/pair*": [a, b]}` | dotted pair `(a . b)` |
| `{"*type/rational*": "1/3"}` | rational |
| `{"*type/complex*": "1+2i"}` | complex |

### Practical Guidance

Use `/interface/lisp-to-json` to generate exact JSON for complex expressions, and use `/interface/json-to-lisp` when debugging client payloads or validating round-trip conversions.
For web clients, prefer object-shaped API queries (`function`, `arguments`, `authentication`) because they remain easier to read and diff than raw array forms, and reserve array-shaped JSON for raw root command calls.
Prefer `*type/string*` for literal strings in JSON payloads whenever you need to avoid ambiguity in type conversion.

A common workflow is: prototype in Lisp, convert to JSON, integrate in client code, then round-trip with `json-to-lisp` during debugging.
Teams that include conversion checks in their release process generally catch payload-shape regressions earlier.
