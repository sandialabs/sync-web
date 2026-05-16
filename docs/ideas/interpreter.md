# Interpreter

The interactive Scheme shell is the terminal-first interface to sync-web — for advanced
users, developers, and autonomous agents operating semi-autonomously.

## What it is

A custom shell where the default environment is sync-web rather than Unix. Instead of
`ls`/`cat`/`grep`, users have Scheme functions that navigate the journal, evaluate S7 code,
query the knowledge graph, and interact with the interface.scm API. Session state
(current path, environment variables, history) lives in-memory for the connection lifetime.

## Transport

SSH with a custom shell is the right substrate:
- Zero client install; `ssh user@syncweb.example.com` works out of the box
- Password auth delivers plaintext over the encrypted tunnel → Kratos login directly
- Public key auth → key stored in Kratos `metadata_private` (works for OIDC/passkey users)
- SSH over WebSocket (wstunnel + ProxyCommand) routes through port 443 if needed
- Readline, history, terminal resize, tmux compatibility all come free from SSH

## Shell capabilities (planned)

- Scheme REPL with journal-awareness: expressions evaluate against the live journal
- Filesystem navigation over journal paths (`cd`, `ls` equivalents as Scheme functions)
- Library search: query the sync-web library index for reusable S7 code
- Policy enforcement: write restrictions enforced server-side, not via convention
- Documentation access: architecture guides and API reference queryable inline

## Agent substrate

The same shell is the substrate for autonomous coding agents:
- Agents connect via SSH or MCP (same underlying capabilities, different interface)
- MCP exposes tools and resources for structured agent access
- The shell exposes the same operations interactively for humans
- LSP (deferred) will add S7 diagnostics and completions for both humans and agents

## Scheme as query language

Scheme functions are the native query interface for the knowledge graph and the journal —
not SPARQL, not Gremlin. The journal already evaluates S7; queries are just functions.
This is the Datomic model (queries as data evaluated by the runtime) but in a language
better suited to it. External query language compatibility (SPARQL) can be layered on top
later if needed for specific integrations.
