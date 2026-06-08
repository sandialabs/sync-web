# Primitive and rootlet inventory

Generated from the vendored sync-web-profile C s7 oracle with:

```scheme
(map car (rootlet))
```

Last refreshed: 2026-06-05. Baseline: s7 10.8 / 15-Jan-2024, built with sync-web behavior flags.

This file is an implementation checklist, not a promise that every upstream binding is exposed to sync-web user code. The Rust port should preserve sync-web-compatible Lisp behavior while constructing root environments from caller-provided primitive availability/blacklist configuration.

## Counts

- Baseline C s7 rootlet bindings before sync-web blacklist: **384**.
- Unique bindings in current sync-web `REMOVE` blacklist: **82**.
- Blacklisted names present in the baseline rootlet: **81**.
- Baseline bindings remaining after current blacklist: **303**.
- sync-web Rust-added host primitives/types listed below are additional to baseline s7.

## Implementation interpretation

Use three distinct concepts:

1. **Implemented internally** — the runtime may need the feature for corpus tests, evaluator implementation, wrappers, or future integration.
2. **Exposed in a root environment** — the binding is available to evaluated Scheme code under the caller's primitive policy.
3. **Omitted/blacklisted** — the binding should be absent or replaced according to caller configuration; many current sync-web-blacklisted upstream features should never be implemented unless explicitly needed.

Important examples:

### Blacklist sync candidates: safe/in-memory ports

The current sync-web blacklist was intentionally conservative and includes several port bindings that are not inherently filesystem/system access. For the Rust port direction, these should be implemented and likely re-included in normal evaluator environments once sync-web's blacklist is revisited:

- `open-input-string` — read Scheme data from an in-memory string; already part of sync-web's current eval wrapper pattern.
- `open-output-string`, `get-output-string` — capture output into memory; aligns with the planned captured-output execution result.
- `display`, `write`, `newline`, `write-char`, `write-string` — safe when directed at interpreter-managed captured/string ports.
- `read-char`, `peek-char`, `read-line`, `read-string`, `read-byte`, `write-byte` — safe for string/captured ports; exposure should depend on supported port types.
- `current-input-port`, `current-output-port`, `current-error-port`, `set-current-error-port` — safe if the Rust interpreter's current ports are interpreter-managed captured/string ports rather than raw process terminal/file handles.
- `close-input-port`, `close-output-port`, `flush-output-port`, `port-closed?` — safe for managed ports.
- `input-port?`, `output-port?` — predicates only.

Do **not** automatically re-include filesystem-backed port bindings:

- `open-input-file`, `open-output-file`
- `call-with-input-file`, `call-with-output-file`
- `with-input-from-file`, `with-output-to-file`
- file metadata helpers such as `port-file`, `port-filename`, `port-line-number`, `port-position` unless they make sense for managed string ports.

Function ports (`open-input-function`, `open-output-function`) are not filesystem access, but they can complicate evaluator reentrancy, metering, and callback behavior. Defer them unless corpus or sync-web integration proves they are needed.

- String input/output ports may be useful internally for `read`, `eval`, captured output, and tests, even if some port bindings remain unavailable to normal sync-web user code.
- `expression->byte-vector`, `byte-vector->expression`, sync-node primitives, crypto primitives, and system-time primitives are sync-web host primitives, not upstream s7 rootlet bindings.
- The Rust port should not recreate the s7 C object/pointer API just because those names appear below; current sync-web blacklists them.

## sync-web-added host primitives

These are currently registered by the Rust journal/evaluator layer around C s7 and need Rust-native equivalents for sync-web integration.

- `expression->byte-vector`
- `byte-vector->expression`
- `hex-string->byte-vector`
- `byte-vector->hex-string`
- `random-byte-vector`
- `print`
- `sync-stub`
- `sync-hash`
- `sync-state`
- `sync-node?`
- `sync-null`
- `sync-null?`
- `sync-pair?`
- `sync-stub?`
- `sync-digest`
- `sync-cons`
- `sync-car`
- `sync-cdr`
- `sync-cut`
- `sync-create`
- `sync-delete`
- `sync-all`
- `sync-call`
- `sync-eval`
- `sync-http`
- `sync-remote`
- `crypto-generate`
- `crypto-sign`
- `crypto-verify`
- `system-time-utc`
- `system-time-unix`

## Current sync-web blacklist (`REMOVE`)

These names are currently overwritten with `*removed*` in `/home/tdinh/projects/sync-web/journal/src/evaluator.rs`. The Rust port should support caller-provided custom blacklists at evaluator construction time rather than hard-coding only this list.

- `*autoload*`
- `*autoload-hook*`
- `*cload-directory*`
- `*features*`
- `*function*`
- `*libraries*`
- `*load-hook*`
- `*load-path*`
- `*stderr*`
- `*stdin*`
- `*stdout*`
- `abort`
- `autoload`
- `c-object-type`
- `c-object?`
- `c-pointer`
- `c-pointer->list`
- `c-pointer-info`
- `c-pointer-type`
- `c-pointer-weak1`
- `c-pointer-weak2`
- `c-pointer?`
- `call-with-current-continuation`
- `call-with-exit`
- `call-with-input-file`
- `call-with-input-string`
- `call-with-output-file`
- `call-with-output-string`
- `call/cc`
- `close-input-port`
- `close-output-port`
- `continuation?`
- `current-error-port`
- `current-input-port`
- `current-output-port`
- `dilambda`
- `dilambda?`
- `dynamic-unwind`
- `dynamic-wind`
- `emergency-exit`
- `exit`
- `flush-output-port`
- `gc`
- `get-output-string`
- `goto?`
- `hook-functions`
- `input-port?`
- `load`
- `make-hook`
- `open-input-file`
- `open-input-function`
- `open-output-file`
- `open-output-function`
- `open-output-string`
- `output-port?`
- `owlet`
- `pair-filename`
- `pair-line-number`
- `peek-char`
- `port-closed?`
- `port-file`
- `port-filename`
- `port-line-number`
- `port-position`
- `profile-in`
- `random`
- `read-char`
- `read-string`
- `read-byte`
- `read-line`
- `require`
- `s7-optimize`
- `set-current-error-port`
- `unlet`
- `with-baffle`
- `with-input-from-file`
- `with-output-to-file`
- `with-output-to-string`
- `write`
- `write-byte`
- `write-char`
- `write-string`

## Baseline C s7 rootlet bindings (384)

Full raw inventory from `(map car (rootlet))` before sync-web removes bindings:

| Binding | Binding | Binding | Binding |
| --- | --- | --- | --- |
| `reader-cond` | `*rootlet-redefinition-hook*` | `*read-error-hook*` | `*error-hook*` |
| `*autoload-hook*` | `*load-hook*` | `*missing-close-paren-hook*` | `*unbound-variable-hook*` |
| `hook-functions` | `make-hook` | `*s7*` | `pi` |
| `*#readers*` | `require` | `*libraries*` | `*autoload*` |
| `*cload-directory*` | `*load-path*` | `*features*` | `profile-in` |
| `quasiquote` | `tree-cyclic?` | `tree-count` | `tree-set-memq` |
| `tree-memq` | `tree-leaves` | `s7-optimize` | `abort` |
| `exit` | `emergency-exit` | `gc` | `type-of` |
| `equivalent?` | `equal?` | `eqv?` | `eq?` |
| `aritable?` | `arity` | `setter` | `dilambda` |
| `*function*` | `funclet` | `procedure-source` | `help` |
| `signature` | `documentation` | `list-values` | `apply-values` |
| `[list*]` | `<list*>` | `values` | `stacktrace` |
| `error` | `throw` | `catch` | `dynamic-unwind` |
| `dynamic-wind` | `map` | `for-each` | `apply` |
| `eval-string` | `eval` | `autoload` | `load` |
| `call-with-exit` | `call-with-current-continuation` | `call/cc` | `cyclic-sequences` |
| `hash-table-value-typer` | `hash-table-key-typer` | `hash-code` | `hash-table-entries` |
| `hash-table-set!` | `hash-table-ref` | `weak-hash-table` | `make-weak-hash-table` |
| `make-hash-table` | `hash-table` | `byte-vector->string` | `string->byte-vector` |
| `byte-vector-set!` | `byte-vector-ref` | `make-byte-vector` | `byte-vector` |
| `int-vector-ref` | `int-vector-set!` | `make-int-vector` | `int-vector` |
| `float-vector-ref` | `float-vector-set!` | `make-float-vector` | `float-vector` |
| `subvector-vector` | `subvector-position` | `subvector` | `vector-typer` |
| `vector` | `make-vector` | `vector-rank` | `vector-dimensions` |
| `vector-dimension` | `vector-set!` | `vector-ref` | `append` |
| `sort!` | `reverse!` | `reverse` | `fill!` |
| `copy` | `length` | `make-list` | `list-tail` |
| `list-set!` | `list-ref` | `list` | `member` |
| `memv` | `memq` | `assoc` | `assv` |
| `assq` | `cdddar` | `cddadr` | `cddddr` |
| `cdaddr` | `cddaar` | `cdadar` | `cdaadr` |
| `cdaaar` | `caddar` | `cadadr` | `cadddr` |
| `caaddr` | `cadaar` | `caadar` | `caaadr` |
| `caaaar` | `cddar` | `cdadr` | `cdddr` |
| `caddr` | `cdaar` | `cadar` | `caadr` |
| `caaar` | `cddr` | `cdar` | `cadr` |
| `caar` | `set-cdr!` | `set-car!` | `cdr` |
| `car` | `cons` | `object->let` | `format` |
| `object->string` | `string` | `substring` | `string-append` |
| `string-upcase` | `string-downcase` | `string-copy` | `string>=?` |
| `string<=?` | `string>?` | `string<?` | `string=?` |
| `string-set!` | `string-ref` | `make-string` | `string-position` |
| `char-position` | `char>=?` | `char<=?` | `char>?` |
| `char<?` | `char=?` | `char-whitespace?` | `char-numeric?` |
| `char-alphabetic?` | `char-lower-case?` | `char-upper-case?` | `integer->char` |
| `char->integer` | `char-downcase` | `char-upcase` | `string->number` |
| `number->string` | `random-state->list` | `nan-payload` | `nan` |
| `integer-decode-float` | `logbit?` | `lognot` | `logxor` |
| `logior` | `logand` | `round` | `truncate` |
| `ceiling` | `floor` | `sqrt` | `atanh` |
| `acosh` | `asinh` | `atan` | `acos` |
| `asin` | `tanh` | `cosh` | `sinh` |
| `tan` | `cos` | `sin` | `angle` |
| `magnitude` | `abs` | `exp` | `ash` |
| `log` | `expt` | `random-state` | `random` |
| `rationalize` | `lcm` | `gcd` | `>=` |
| `<=` | `>` | `<` | `=` |
| `modulo` | `remainder` | `quotient` | `max` |
| `min` | `/` | `*` | `-` |
| `+` | `complex` | `nan?` | `infinite?` |
| `negative?` | `positive?` | `zero?` | `odd?` |
| `even?` | `denominator` | `numerator` | `imag-part` |
| `real-part` | `with-output-to-file` | `with-output-to-string` | `call-with-output-file` |
| `call-with-output-string` | `with-input-from-file` | `with-input-from-string` | `call-with-input-file` |
| `call-with-input-string` | `read` | `read-string` | `read-line` |
| `write-byte` | `read-byte` | `write-string` | `write-char` |
| `peek-char` | `read-char` | `display` | `write` |
| `newline` | `open-output-function` | `open-input-function` | `get-output-string` |
| `open-output-string` | `open-input-string` | `open-output-file` | `open-input-file` |
| `flush-output-port` | `close-output-port` | `close-input-port` | `set-current-error-port` |
| `current-error-port` | `current-output-port` | `current-input-port` | `port-closed?` |
| `pair-filename` | `pair-line-number` | `port-filename` | `port-line-number` |
| `port-position` | `port-file` | `c-pointer->list` | `c-pointer-weak2` |
| `c-pointer-weak1` | `c-pointer-type` | `c-pointer-info` | `c-pointer` |
| `c-object-type` | `defined?` | `provide` | `provided?` |
| `iterator-at-end?` | `iterator-sequence` | `iterate` | `make-iterator` |
| `let-set!` | `let-ref` | `openlet` | `coverlet` |
| `owlet` | `inlet` | `cutlet` | `varlet` |
| `sublet` | `funclet?` | `unlet` | `curlet` |
| `rootlet` | `outlet` | `keyword->symbol` | `symbol->keyword` |
| `string->keyword` | `constant?` | `immutable?` | `immutable!` |
| `symbol->dynamic-value` | `symbol->value` | `symbol` | `string->symbol` |
| `symbol->string` | `symbol-table` | `gensym` | `bignum` |
| `bignum?` | `not` | `goto?` | `weak-hash-table?` |
| `subvector?` | `c-object?` | `unspecified?` | `undefined?` |
| `null?` | `sequence?` | `proper-list?` | `boolean?` |
| `dilambda?` | `procedure?` | `continuation?` | `hash-table?` |
| `byte-vector?` | `int-vector?` | `float-vector?` | `vector?` |
| `pair?` | `list?` | `string?` | `char?` |
| `random-state?` | `rational?` | `complex?` | `float?` |
| `real?` | `number?` | `byte?` | `integer?` |
| `eof-object?` | `output-port?` | `input-port?` | `c-pointer?` |
| `macro?` | `iterator?` | `openlet?` | `let?` |
| `keyword?` | `gensym?` | `syntax?` | `symbol?` |
| `else` | `*stderr*` | `*stdout*` | `*stdin*` |

## Baseline rootlet after current sync-web blacklist (303)

This is a useful starting checklist for a sync-web-compatible default root environment, before adding sync-web host primitives and before applying any caller-specific stricter blacklist.

| Binding | Binding | Binding | Binding |
| --- | --- | --- | --- |
| `reader-cond` | `*rootlet-redefinition-hook*` | `*read-error-hook*` | `*error-hook*` |
| `*missing-close-paren-hook*` | `*unbound-variable-hook*` | `*s7*` | `pi` |
| `*#readers*` | `quasiquote` | `tree-cyclic?` | `tree-count` |
| `tree-set-memq` | `tree-memq` | `tree-leaves` | `type-of` |
| `equivalent?` | `equal?` | `eqv?` | `eq?` |
| `aritable?` | `arity` | `setter` | `funclet` |
| `procedure-source` | `help` | `signature` | `documentation` |
| `list-values` | `apply-values` | `[list*]` | `<list*>` |
| `values` | `stacktrace` | `error` | `throw` |
| `catch` | `map` | `for-each` | `apply` |
| `eval-string` | `eval` | `cyclic-sequences` | `hash-table-value-typer` |
| `hash-table-key-typer` | `hash-code` | `hash-table-entries` | `hash-table-set!` |
| `hash-table-ref` | `weak-hash-table` | `make-weak-hash-table` | `make-hash-table` |
| `hash-table` | `byte-vector->string` | `string->byte-vector` | `byte-vector-set!` |
| `byte-vector-ref` | `make-byte-vector` | `byte-vector` | `int-vector-ref` |
| `int-vector-set!` | `make-int-vector` | `int-vector` | `float-vector-ref` |
| `float-vector-set!` | `make-float-vector` | `float-vector` | `subvector-vector` |
| `subvector-position` | `subvector` | `vector-typer` | `vector` |
| `make-vector` | `vector-rank` | `vector-dimensions` | `vector-dimension` |
| `vector-set!` | `vector-ref` | `append` | `sort!` |
| `reverse!` | `reverse` | `fill!` | `copy` |
| `length` | `make-list` | `list-tail` | `list-set!` |
| `list-ref` | `list` | `member` | `memv` |
| `memq` | `assoc` | `assv` | `assq` |
| `cdddar` | `cddadr` | `cddddr` | `cdaddr` |
| `cddaar` | `cdadar` | `cdaadr` | `cdaaar` |
| `caddar` | `cadadr` | `cadddr` | `caaddr` |
| `cadaar` | `caadar` | `caaadr` | `caaaar` |
| `cddar` | `cdadr` | `cdddr` | `caddr` |
| `cdaar` | `cadar` | `caadr` | `caaar` |
| `cddr` | `cdar` | `cadr` | `caar` |
| `set-cdr!` | `set-car!` | `cdr` | `car` |
| `cons` | `object->let` | `format` | `object->string` |
| `string` | `substring` | `string-append` | `string-upcase` |
| `string-downcase` | `string-copy` | `string>=?` | `string<=?` |
| `string>?` | `string<?` | `string=?` | `string-set!` |
| `string-ref` | `make-string` | `string-position` | `char-position` |
| `char>=?` | `char<=?` | `char>?` | `char<?` |
| `char=?` | `char-whitespace?` | `char-numeric?` | `char-alphabetic?` |
| `char-lower-case?` | `char-upper-case?` | `integer->char` | `char->integer` |
| `char-downcase` | `char-upcase` | `string->number` | `number->string` |
| `random-state->list` | `nan-payload` | `nan` | `integer-decode-float` |
| `logbit?` | `lognot` | `logxor` | `logior` |
| `logand` | `round` | `truncate` | `ceiling` |
| `floor` | `sqrt` | `atanh` | `acosh` |
| `asinh` | `atan` | `acos` | `asin` |
| `tanh` | `cosh` | `sinh` | `tan` |
| `cos` | `sin` | `angle` | `magnitude` |
| `abs` | `exp` | `ash` | `log` |
| `expt` | `random-state` | `rationalize` | `lcm` |
| `gcd` | `>=` | `<=` | `>` |
| `<` | `=` | `modulo` | `remainder` |
| `quotient` | `max` | `min` | `/` |
| `*` | `-` | `+` | `complex` |
| `nan?` | `infinite?` | `negative?` | `positive?` |
| `zero?` | `odd?` | `even?` | `denominator` |
| `numerator` | `imag-part` | `real-part` | `with-input-from-string` |
| `read` | `display` | `newline` | `open-input-string` |
| `defined?` | `provide` | `provided?` | `iterator-at-end?` |
| `iterator-sequence` | `iterate` | `make-iterator` | `let-set!` |
| `let-ref` | `openlet` | `coverlet` | `inlet` |
| `cutlet` | `varlet` | `sublet` | `funclet?` |
| `curlet` | `rootlet` | `outlet` | `keyword->symbol` |
| `symbol->keyword` | `string->keyword` | `constant?` | `immutable?` |
| `immutable!` | `symbol->dynamic-value` | `symbol->value` | `symbol` |
| `string->symbol` | `symbol->string` | `symbol-table` | `gensym` |
| `bignum` | `bignum?` | `not` | `weak-hash-table?` |
| `subvector?` | `unspecified?` | `undefined?` | `null?` |
| `sequence?` | `proper-list?` | `boolean?` | `procedure?` |
| `hash-table?` | `byte-vector?` | `int-vector?` | `float-vector?` |
| `vector?` | `pair?` | `list?` | `string?` |
| `char?` | `random-state?` | `rational?` | `complex?` |
| `float?` | `real?` | `number?` | `byte?` |
| `integer?` | `eof-object?` | `macro?` | `iterator?` |
| `openlet?` | `let?` | `keyword?` | `gensym?` |
| `syntax?` | `symbol?` | `else` |  |

