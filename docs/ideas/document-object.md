# Document Object

## Goal

Introduce a durable `document.scm` object as the journal-level representation for
stored documents. The document object should carry document value logic, optional
metadata logic, and enough embedded code for future ledger versions to read old
documents without knowing their raw `sync-cons` layout.

Metadata is intended as a general extension surface. Initial motivating consumers
include future WebDAV/file metadata, subject-primary knowledge graph annotations, and
other application-level tools. The document layer should not bake in those application
schemas.

## Direction

Use an embedded sync-eval-compatible document object instead of making `tree.scm`
understand metadata directly.

Near-term architecture:

- `tree.scm` remains the path-addressed Merkle map.
- `document.scm` owns document value encoding, metadata encoding, and metadata update
  semantics.
- `ledger.scm` stores document objects as tree values and delegates document-specific
  behavior to those objects.
- `standard.scm` continues to provide the generic deep traversal, slice, prune, and
  merge machinery.

This is an intermediate step toward a possible future where directories are also
objects and `standard.scm` can traverse object boundaries more intelligently. That
larger tree refactor is intentionally out of scope here.

## Rationale

The important compatibility boundary should be the document object API, not a raw
pair layout in the cryptographic tree.

Because sync nodes carry executable object logic, each document can bring along the
logic needed to interpret its own stored representation. Future `ledger.scm` versions
should be able to call stable document methods such as `value`, `meta`, `get`, and
`set!` rather than knowing whether metadata happens to live in the car or cdr of a
specific historical pair shape.

This keeps `tree.scm` focused on Merkle path storage and moves document semantics to
a layer that can evolve independently.

## Object API

The document object API should be small and stable:

```scheme
(*type*)       ; document
(*api*)        ; stable list of supported methods
(value)        ; decoded document value
(meta)         ; decoded metadata expression, () when absent/empty
(get key)      ; generic access to value/meta
(set! key val) ; update value/meta and return #t
(slice! key)   ; proof projection for value/meta
(prune! key)   ; proof removal for value/meta
```

Optional convenience methods may be useful:

```scheme
(patch-meta! patch) ; dictionary-style metadata patch
(empty-meta?)       ; #t when metadata is semantically empty
```

The generic `get` and `set!` methods make the object compatible with existing
`standard.scm` deep traversal patterns. Public APIs may still expose a cleaner
`meta?` option, but internally paths like `value` and `meta` can be ordinary object
segments.

## Stored Shape

A first implementation can use a simple shape:

```scheme
(sync-cons document-code (sync-cons meta-node value-node))
```

That shape is private to the document object's embedded code. Other layers should
not depend on it directly.

The object should distinguish byte-vector values from expression values efficiently.
A reasonable internal encoding is:

```scheme
#u(0 ...) ; raw byte-vector document value
#u(1 ...) ; expression->byte-vector encoded Scheme expression
```

Metadata can initially be stored as an expression-encoded association list. If later
metadata consumers need a more efficient binary representation, a new document object
version can carry that logic without requiring the ledger to understand both layouts.

## Metadata Shape

Metadata should use dictionary update semantics.

The metadata payload is an association list keyed by symbols. Each key owns an
arbitrary Scheme-expression value:

```scheme
((webdav ((content-type "text/plain")
          (display-name "notes.txt")))
 (rdf    (((predicate (*state* schema type))
           (object (*state* schema Note))))))
```

The document layer only treats this as a symbol-keyed metadata dictionary. `webdav`,
`rdf`, or any future key is a convention owned by higher layers.

This avoids requiring independent metadata writers to download, understand, and
rewrite metadata owned by other applications.

## Read Semantics

Public `get` and `resolve` should accept an optional `meta?` argument.

Default behavior returns only document content:

```scheme
(get path)
(get path (meta? #f))
;; => current content shape
```

Metadata-aware reads return a public envelope:

```scheme
(get path (meta? #t))
;; => ((content value) (meta metadata-alist))
```

If no metadata is stored, `meta` returns `()`:

```scheme
((content "hello") (meta ()))
```

Proof behavior should remain an emergent property of traversing the requested object
representation. If a request asks for document value, the proof covers value. If it
asks for metadata, the proof covers metadata. There should not be separate proof
policy for metadata.

## Write Semantics

Public `set!` should accept an optional `meta` argument. There should not be a
separate metadata-only public endpoint if the same mental model can stay clear.

Public API rules:

- `value` present: write that value, including `#f` if explicitly supplied.
- `value` omitted and `meta` present: metadata-only update to an existing document.
- both `value` and `meta` omitted: error.
- `meta` omitted or `meta ()`: leave existing metadata unchanged.
- `meta #f`: clear all metadata.
- `meta ((key value) ...)`: patch only the listed metadata keys.
- `meta ((key #f) ...)`: delete only the listed metadata keys.

Examples:

```scheme
((function set!)
 (arguments ((path ((*state* alice doc)))
             (value "hello"))))
;; write content, preserve existing metadata

((function set!)
 (arguments ((path ((*state* alice doc)))
             (meta ((rdf (((predicate (*state* schema type))
                           (object (*state* schema Note))))))))))
;; metadata-only update; content unchanged

((function set!)
 (arguments ((path ((*state* alice doc)))
             (value "hello")
             (meta ((webdav ((content-type "text/plain"))))))))
;; write content and patch webdav metadata

((function set!)
 (arguments ((path ((*state* alice doc)))
             (meta ((rdf #f))))))
;; delete only rdf metadata

((function set!)
 (arguments ((path ((*state* alice doc)))
             (meta #f))))
;; clear all metadata
```

The document object should own metadata patch mechanics. Ledger/interface code should
normalize public call shape, then delegate the actual metadata update to the document
object.

## Missing Values

Metadata-only writes require an existing document.

If `value` is omitted and the target path resolves to `(nothing)`, `(unknown)`, or a
directory listing, the write should fail. Metadata annotates a document; it must not
create a document or imply that a path prefix is a document.

To create a document with metadata, callers must provide both `value` and `meta`.

## Interface Boundary

The interface layer must normalize public calls before passing them downward.

This is necessary because only `interface.scm` has the raw `arg-list` needed to
distinguish an omitted `value` argument from an explicit `value #f`.

Suggested flow:

1. Inspect raw `arg-list`.
2. Determine whether `value` was present.
3. Determine whether `meta` was present.
4. Reject calls with neither `value` nor `meta`.
5. If `value` is omitted, read the current document and reject missing, unknown, or
   directory results.
6. Pass a normalized document update downward.

Lower layers should not need sentinel values to recover whether the public user
omitted `value`.

## Tree Boundary

`tree.scm` should not become the owner of metadata.

In this design, tree only needs to continue supporting sync-node values through its
existing value storage behavior. It should remain responsible for key hashing,
directory path lookup, raw slicing, pruning, merging, and directory listings.

Document-oriented behavior belongs in `document.scm`, including:

- value decoding
- metadata decoding
- metadata patching
- empty metadata behavior
- byte-vector versus expression value encoding
- object-level `get`, `set!`, `slice!`, and `prune!`

Directory metadata remains out of scope. Directories are path prefixes, not documents.
If a client wants directory-like metadata, it should write a normal document at a
conventional path such as a special directory marker, following the same pattern used
by filesystem-oriented interfaces.

## Performance Notes

Making documents objects adds overhead compared with plain tree values:

- each document has additional sync-node structure;
- cold reads may need `sync-eval`;
- proofs and serialized responses include the document object boundary;
- large byte-vector values need internal tagging to avoid expression-vector expansion.

The journal's strict `sync-eval` cache makes hot object-method calls less concerning,
but it does not remove storage, proof, or cold-read overhead.

This is a deliberate architecture tradeoff. The payoff is a durable document protocol,
less document-specialized logic in `tree.scm`, and future compatibility through
embedded object behavior.

## Non-Goals

This document does not define:

- WebDAV implementation details.
- RDF query language or RDF-specific storage schema.
- Global metadata indexes.
- Directory metadata.
- Object-per-directory tree refactoring.
- Path-misdirection schemes such as storing `doc:value` and `doc:meta` as sibling
  tree paths.

The point is to introduce a stable document abstraction that higher layers can use
without spreading metadata conventions across tree paths or hard-coding raw document
layouts into future ledger implementations.
