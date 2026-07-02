# Service path symbols

## Problem

Sync Web ledger paths are Scheme paths. Public path segments should be atomic Scheme symbols, but WebDAV file names and Explorer display names are arbitrary path strings. Some names, such as names containing spaces, cannot be represented as simple unescaped Scheme identifier tokens without relying on implementation-specific printed forms such as `(symbol "...")`.

Those constructor-style printed forms should not become part of the public ledger path contract. The ledger should receive ordinary symbol atoms. Services that expose foreign naming systems should translate names at their own boundary.

## Direction

Use a service-side percent-escape convention for path segments that are not safe unescaped R7RS identifiers.

- Ledger/core contract: path segments are atomic symbols. Ledger does not interpret percent escapes.
- Service contract: arbitrary file/display names are converted to valid unescaped Scheme identifier symbols before gateway calls.
- Display contract: WebDAV and Explorer decode valid percent escapes from ledger directory listings when presenting file names.
- Case is preserved.
- Invalid/non-atomic path entries such as `(symbol "...")` are not supported as reachable public paths.

## R7RS identifier baseline

A WebDAV segment may pass through unchanged when it is an unescaped R7RS-style identifier:

- initial: `A-Z`, `a-z`, `!`, `$`, `%`, `&`, `*`, `/`, `:`, `<`, `=`, `>`, `?`, `^`, `_`, `~`
- subsequent: initial plus `0-9`, `+`, `-`, `.`, `@`
- peculiar identifiers: `+`, `-`, `...`, and `->` followed by zero or more subsequent characters

Everything else is escaped by UTF-8 byte as `%HH` using uppercase hexadecimal. For WebDAV-created names, literal `%` is always escaped as `%25`, so decoded WebDAV names round-trip without collisions.

Examples:

| Display/WebDAV name | Ledger symbol |
| --- | --- |
| `sync-node?` | `sync-node?` |
| `*` | `*` |
| `<` | `<` |
| `New folder` | `New%20folder` |
| `a%b` | `a%25b` |
| `a%20b` | `a%2520b` |
| `é` | `%C3%A9` |

## Service behavior

- Encode path segments before constructing gateway JSON paths.
- Decode valid `%HH` sequences in directory listing names before constructing user-visible names or WebDAV hrefs.
- Preserve the encoded segment internally for later API calls; decoded names are presentation only.
- Leave invalid percent sequences literal while decoding for display.
- Do not normalize case.
- Do not reject user names merely because they need escaping.

## Notes

This convention is not a ledger semantic. A frontend that does not implement the convention still sees readable, valid Scheme symbols; it just may show escaped names such as `New%20folder`.
