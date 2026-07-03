# Contributing

This repository is the Synchronic Web monorepo. Prefer small, focused pull requests with clear testing notes.

## Versioning and releases

- Pull requests that change the platform/runtime should bump the root `VERSION`.
- Agent Recorder-only release-worthy changes should bump `tools/agent-recorder/Cargo.toml` instead of the root `VERSION`.
- After changing root `VERSION`, run:
  ```sh
  node scripts/sync-platform-package-versions.js --write
  ```
  and commit the resulting package metadata updates.
- Only bump `journal/Cargo.toml` when the `journal-sdk` crate itself changes.
- Root `VERSION` is the source of truth for platform container tags and ledger binary releases. Agent Recorder has its own release/version stream. Do not mutate existing release assets; publish a new patch version instead.

## Testing

Run the checks relevant to your change and list them in the PR body. Common checks:

```sh
./records/tests/test.sh ./journal/target/debug/journal-sdk
node scripts/sync-platform-package-versions.js
cd services/explorer && npm test -- --watchAll=false && npm run build
cd services/gateway && npm test -- --runInBand
cd docs/info && npm run build
cd tools/agent-recorder && cargo test
```

Use `~/.cargo/bin/cargo` if `cargo` is not on your local `PATH`.

## Records and Scheme changes

- Read `records/LANGUAGE.md` before changing `records/lisp/*.scm`.
- Preserve public method names, argument shapes, return shapes, and sentinel behavior unless the change is intentionally breaking.
- Keep durable semantic state in sync-node object state, not only in live Scheme bindings.
- Mutating a live object does not persist it into a parent/root automatically; persist the returned object node where needed.
- Add or update `records/tests` for public method changes, state-layout changes, authorization changes, bridge behavior, and persistence-sensitive mutations.

## Deployment-sensitive changes

- Treat `deploy/compose/general/` as a production path, not just local scaffolding.
- Avoid documenting or scripting `down -v` casually; it destroys deployment data.
- Existing journal databases require `JOURNAL_UPDATE=1` to load updated mounted record code. Use it intentionally and turn it back off after the update.
- Runtime-managed admin state must not be reset by record updates; environment admin lists are seed inputs for fresh installs.
- Public deployments should set `ORIGIN`, and may set `INTERFACE` and `JOURNAL_NAME`; never derive public identity fields from secrets.
- Preserve both Docker and Podman/Compose-compatible workflows where practical.

## Pull request notes

A useful PR body includes:

- **Summary** — what changed and why.
- **Testing** — commands run, or `Not run` with reason.
- **Notes/Risks** — deployment, migration, compatibility, security, or follow-up concerns.
