## Summary

-
-
-

## Testing

- [ ] `node scripts/sync-platform-package-versions.js`
- [ ] Records tests, if records changed: `./records/tests/test.sh ./journal/target/debug/journal-sdk`
- [ ] Explorer tests/build, if Explorer changed: `cd services/explorer && npm test -- --watchAll=false && npm run build`
- [ ] Gateway tests, if Gateway changed: `cd services/gateway && npm test -- --runInBand`
- [ ] Docs build, if docs changed: `cd docs/info && npm run build`
- [ ] Agent recorder tests, if agent-recorder changed: `cd tools/agent-recorder && cargo test`

Other checks run:

```text

```

## Versioning

- [ ] Root `VERSION` bumped, or not needed because:
- [ ] Package versions synced with `node scripts/sync-platform-package-versions.js --write`, or not needed because:
- [ ] `journal/Cargo.toml` version changed only if the journal-sdk crate changed, or not applicable
- [ ] `tools/agent-recorder/Cargo.toml` version bumped for Agent Recorder-only release-worthy changes, or not applicable

## Deployment / records impact

- [ ] No record changes, or existing deployments require `JOURNAL_UPDATE=1`
- [ ] No runtime-managed admin/config state will be reset by updates
- [ ] Public deployment variables and defaults remain safe (`ORIGIN`, `INTERFACE`, `JOURNAL_NAME`, secrets)
- [ ] No destructive volume/data commands are added or documented without explicit warnings

## Security / release notes

- [ ] No secrets, private data, local databases, generated caches, or runtime artifacts committed
- [ ] Release/container/binary artifact implications considered
- [ ] Security-sensitive changes are described here or handled privately via `SECURITY.md`

## Notes / risks

-
