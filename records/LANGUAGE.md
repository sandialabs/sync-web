# Records Language

- Scope: object language used by active code in `records/lisp`.
- Audience: class authors working at the `standard.scm` object layer.

## Context

### State Management

- All Lisp code runs inside the journal SDK's s7 evaluator.
- Sync Web state is built from sync nodes, byte vectors, encoded expressions, and ordinary Scheme values.
  - `sync-cons`, `sync-car`, and `sync-cdr` are the core node constructors and accessors.
  - `(sync-null)` represents empty structure.
  - `sync-cut` creates a digest-preserving stub for unavailable structure.
  - `sync-digest` computes the digest used for equality, proofs, and merges.
  - `expression->byte-vector` and `byte-vector->expression` encode and decode ordinary Scheme data.
- Durable semantic state must live in sync-node object state, not only in process-local Scheme bindings.
- Existing tagged encodings must be preserved when a class distinguishes raw bytes from expressions.
- Stubs represent unknown data, not empty data.
- Record code is normally installed beneath a root object.
- The root object stores class definitions, object instances, and runtime configuration by path.
- `interface.scm` is the active deployment entry point that installs and wires root, standard, tree, chain, and ledger code for the current stack.
- Root paths are invocation context for object code, not the object model itself.
- Object classes should not assume a particular root path unless that path is part of their documented contract.

### Network Operations

- Network-facing operations are coordinated by `interface.scm`, not by ordinary object methods.
- `sync-remote` and related journal/network primitives should be used to prepare remote data before durable mutation begins.
- Object mutation methods should accept already-fetched remote data as arguments.
- Do not fetch external data while mutating durable object state.
- This separation keeps standard object methods inside the deterministic method-dispatch protocol and avoids mixing remote side effects with state updates.
- Bridge operations follow this pattern: interface code fetches `info`, `synchronize`, or `trace` data from a peer, then ledger methods apply the prepared payload.

### Execution Flow

- External calls enter through the user-facing interface and eventually evaluate record code inside the journal runtime.
- Interface code is responsible for authentication, authorization, batching, root-object persistence, and orchestration of network fetches.
- Object classes should expose ordinary methods and avoid depending on a particular external call shape.
- `(sync-eval object-node #f)` creates a live callable object from stored node state.
- Mutating a live object changes that live object's current state, but it does not automatically update parent objects or root storage.
- Interface and ledger code must persist mutated objects explicitly by storing their returned `(object)` node back into the containing object or root path.
- When a workflow combines remote data and mutation, fetch or compute the remote data first, then call mutation methods with that data, then persist the mutated object node.

## Objects

### Core Definition

- The core portable unit of data in the Synchronic Web is a self-describing structure called an object.
- Objects are representable as a sync node that contains executable code on the left and durable state on the right.
- `(sync-eval object-node #f)` returns the callable object.
- Calling that object with no argument returns its current node representation.
- Objects may also accept arguments, but argument behavior is object-specific.
- The no-argument call is the only portable behavior code should assume when invoking an arbitrary object without a documented protocol.
- Do not assume arbitrary objects were created by the current `standard.scm`.
- Object-specific functionality is defined by the object's own logic and any documented public protocol it implements.

### Standard Shape

- `standard.scm` turns a class form into an object node.
- Classes are written as `(define-class (...))` forms.
- Methods are written as `(define-method (...))` forms.
- A class body may contain method definitions and optional doc strings; other top-level forms are not accepted by `standard.scm` class construction.
- Constructor behavior belongs in `*init*`.
- Use `*init*` to establish durable object state.
- Internal helper methods use a `~` prefix.
- Mutating methods use a `!` suffix when they update durable object state.
- Standard objects dispatch symbol arguments to methods or built-in object operations.
- Standard objects read internal state when called with a list argument.
- Standard method dispatch packages the current object state and explicit arguments into a strict `sync-eval` call.
- Standard method calls are deterministic, including nested standard method calls, because `standard.scm` routes method execution through strict object evaluation.
- Time, random data, remote responses, and other external inputs should be prepared outside object mutation methods and passed as explicit arguments.
- `self` inside a method is the callable object itself.
- `(self)` returns the current object node.
- `(self path)` reads internal state at `path`.
- `(set! (self path) value)` updates internal durable state at `path`.
- Public object composition should use documented public methods.
- Always define and use methods to mutate standard-defined objects; do not rely on their raw `sync-cons`, `sync-car`, or `sync-cdr` shape.
- Only documented public methods should be treated as stable protocol.
- Private `~` methods are implementation details unless documented as a shared internal protocol.
- Do not call another object's private `~` methods from peer classes or interface/orchestration code; add or use an explicit public method when cross-object coordination is needed.

### Standard Usage

- `((standard 'make) class)` creates an uninitialized object shell.
- `((standard 'init) class . args)` creates an object shell and calls `*init*` when present.
- Use `(sync-eval object-node #f)` when a live object closure is needed.
- Invoke a method by first dispatching on its symbol: `((object 'method) arg ...)`.
- A live object and the node that produced it can diverge after mutation.
- Persist `(object)` when the mutated object state should survive outside the current call.
- If a method mutates a child object, store the child's `(child)` node back into the
  parent object when persistence is intended.
- `(self path)` is state access, not method dispatch.
- `(self 'method)` is method dispatch, not state access.
- Symbols and lists have different dispatch meanings when passed to an object.
- Do not assume another object uses the same internal state paths.
- State layout is part of a class's compatibility contract; changing it requires migration or explicit compatibility handling.
- Prefer ordinary Scheme values for public method arguments and returns unless the
  method is explicitly object-oriented.
- Use existing sentinels consistently:
  - `(nothing)` means intentionally absent or deleted content.
  - `(unknown)` means unavailable or unresolved content.
- Do not treat absent, unknown, empty, and false as interchangeable.

### Standard Operations

- `standard.scm` provides shared operations for composing object methods across nested objects.
  - `deep-get` composes through object `get` methods and must not continue traversal through non-object values.
  - `deep-set!` composes through object `get` and `set!` methods.
  - `deep-slice!` composes through object `get`, `slice!`, and `set!` methods.
  - `deep-prune!` composes through object `get`, `prune!`, and `set!` methods.
  - `deep-copy!` is `deep-get` followed by `deep-set!`.
  - `deep-call` evaluates a callback against an object at a path without rebuilding parent state.
  - `deep-call!` evaluates a callback and rebuilds parent state with the mutated child.
  - `deep-merge!` merges sync-node structures only when digests match.
- Classes that participate in deep operations should implement the relevant public
  methods with compatible semantics.
- Digest-preserving operations must verify that the digest is unchanged before replacing proof or stub state.

## Development

### Conventions

- Keep path conventions class-specific unless they are documented as a shared object protocol.
- Validate path shape before mutation when a class accepts paths.
- Do not encode durable semantic state only in path conventions when it belongs in object state.
- Do not use sync-node values as keys unless the class explicitly supports that.
- Keep class code in active files under `records/lisp`; use `records/lisp/archive` only as reference material.
- Prefer concise documentation comments or docstrings on public classes and methods that describe purpose, arguments, return shape, and mutation behavior.

### Style

- Prefer `let` and `let*` for local bindings instead of nested internal `define` forms.
- Avoid breaking out helper functions unless the logic is reused or the helper names a meaningful protocol step.
- Prefer one obvious way to express each operation; avoid parallel aliases, duplicate paths, and repeated implementation logic.
- Prefer `define*` when optional arguments or argument defaults make the call shape clearer.
- Prefer `case` or `cond` for dispatch and validation branches instead of long chains of nested conditionals.
- Use named `let` loops for local recursion over paths, keys, chains, and serialized structures.
- Keep mutation sequences close to the method that owns the state, especially when the order of state updates matters.
- Use quasiquote for constructing paths, query forms, and serialized expressions when it makes the intended shape explicit.
- Avoid `catch`-based control flow; prefer explicit validation, branching, and sentinel values for expected cases.

### Change Checklist

- Read the class being changed and any class it composes through public methods.
- Preserve public method names, argument shapes, return shapes, and sentinel behavior unless the change is intentionally breaking.
- Preserve internal state layout, or provide explicit migration/compatibility handling.
- Update docstrings when public class or method behavior changes.
- Update or add tests for public method changes, state-layout changes, deep-operation behavior, and persistence-sensitive mutations.

### Tests

- Active tests live in `records/tests`.
- The test harness loads active source files from `records/lisp`.
- Tests are source-driven Scheme lambdas evaluated by the journal SDK.
- Add or update tests when changing public object methods, state layout, or deep-operation behavior.
- Test both the live object result and the persisted `(object)` node when mutation semantics matter.
- Treat interpreter-level `(error ...)` output as test failure unless the test explicitly expects an error result.

### Examples

- Minimal class shape using the common `get`/`set!` protocol:

  ```scheme
  (define-class (cell)
    ;; Cell class stores one value under the public key `value`.

    (define-method (*init* self (initial '(nothing)))
      ;; Initialize durable state.
      ;;   Args:
      ;;     initial: initial value to store.
      ;;   Returns:
      ;;     boolean: #t after setting state.
      (set! (self '(1)) (expression->byte-vector initial)))

    (define-method (get self key)
      ;; Return the stored value for a supported key.
      ;;   Args:
      ;;     key (symbol): supported key is `value`.
      ;;   Returns:
      ;;     any: stored value.
      (case key
        ((value) (byte-vector->expression (self '(1))))
        (else (error 'key-error "Cell does not contain key"))))

    (define-method (set! self key value)
      ;; Update durable state for a supported key.
      ;;   Args:
      ;;     key (symbol): supported key is `value`.
      ;;     value: replacement value.
      ;;   Returns:
      ;;     boolean: #t after setting state.
      (case key
        ((value) (set! (self '(1)) (expression->byte-vector value)))
        (else (error 'key-error "Cell does not contain key")))))
  ```

- Active classes are the best reference points for different parts of the object layer.
- `tree.scm`: best example for path-oriented storage. It shows key encoding, value tagging, sentinels, directory validation, and the `get`/`set!`/`slice!`/`prune!`/`merge!` method family expected by deep operations.
- `linear-chain.scm`: simplest chain example. It is useful for understanding the chain protocol (`size`, `index`, `get`, `previous`, `digest`, `push!`, `slice!`, `prune!`) without the complexity of the log-structured implementation.
- `log-chain.scm`: production-oriented chain example. It implements the same public chain protocol as `linear-chain.scm`, but uses a log-structured layout for more compact history/proof behavior.
- `ledger.scm`: example of object composition and journal-level semantics. It wires together `standard`, tree, and chain objects; manages staged state, permanent history, temporary retention windows, pinning, signatures, bridge state, and prepared remote synchronization payloads. It also demonstrates fixed internal field paths through `~field!`, so treat layout changes as migration-sensitive.

### Performance Considerations

- `sync-eval` creates a live object closure; avoid repeated evaluation when one live object can be reused safely.
- Deep operations may repeatedly evaluate nested objects.
- Serialization, slicing, pruning, and merging can traverse large sync-node structures.
- Store byte data directly as byte vectors when possible; wrapping bytes as encoded expressions is less compact.
- Sync-node structures are immutable and effectively copy-on-write, so copying or moving existing structure is usually cheaper than it may appear.
- Mutating workflows that involve network operations are especially expensive because remote I/O and journal writes must be carefully sequenced, and committed writes are serialized through the journal.

