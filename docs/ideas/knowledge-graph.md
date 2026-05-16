# Knowledge Graph

## Model

Subject-primary RDF (Linked Data conventions): each journal path is a node; its outgoing
edges live at that path. This is the mainstream deployed form of RDF — not a compromise.

- Nodes are fully qualified journal paths; subjects, predicates, and objects are all paths
- Discovery is natural: go to a path, find its edges, follow them to other paths
- Avoids RDF's AAA (Anyone can say Anything About Anything) awkwardness — paths have
  ownership; content at a path is that path's content
- Named graphs map to journal paths; SPARQL `FROM NAMED` = path lookup, no full scan

## Encoding

Structured Scheme expression at the subject path, consistent with existing envelope patterns:

```scheme
(((predicate (*state* schema knows))  (object (*state* people bob))   (since 2024))
 ((predicate (*state* schema knows))  (object (*state* people carol)))
 ((predicate (*state* schema type))   (object (*state* schema Person))))
```

- Lightest weight of the options considered; no new object type required
- RDF-star-style annotation: add fields to edge entries inline — no structural change needed
- Wildcard queries over a subject's edges are a linear scan of the list, which is fine for
  the common case (subjects don't have millions of edges)

## Query language

Scheme functions are the native query layer — already available, no new runtime, natural
traversal from a subject path. A query is a Scheme function that starts at a path, follows
edges, filters, and recurses.

SPARQL is antithetical to subject-primary / node-first navigation: it assumes a global
collection of triples and requires secondary indexes that fight the distributed DAG shape.
Deferred unless a concrete external tool integration requires it.

## Triples vs quads

Subject-primary collapses the named graph into the path itself — no fourth element needed.
Quads become relevant only for cross-source provenance, which RDF-star handles more
elegantly anyway (`<<alice knows bob>> assertedBy charlie`).

## Prior art

`records/lisp/archive/rdf.scm`: a full 8-index triple store implemented as a sync-web
transition function using verifiable HAMTs — exactly the right design, obsoleted by new
object semantics. Worth reviving when structured knowledge queries become a priority.
