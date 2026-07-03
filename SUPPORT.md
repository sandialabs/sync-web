# Support

Use GitHub Issues for ordinary bug reports, feature requests, and questions.

For suspected vulnerabilities or sensitive security concerns, do not open a public issue. Follow `SECURITY.md` instead.

## Helpful information for bug reports

Include the details that apply:

- Sync Web version from `VERSION`, image tags, release tag, or commit SHA
- deployment mode: general Compose stack, ledger binary, local test compose, or another setup
- container runtime and host OS, for example Docker, Podman, Linux, macOS, or Windows
- affected service or area, such as journal, records, gateway, explorer, WebDAV file-system, ledger binary, or agent-recorder
- commands run and observed output
- relevant logs with secrets and private data redacted
- whether an existing journal database was updated with `JOURNAL_UPDATE=1`

## Useful references

- Records/Scheme object language: `records/LANGUAGE.md`
- General Compose deployment: `deploy/compose/general/README.md`
- Ledger binary deployment: `deploy/bin/ledger/README.md`
- Agent recorder: `tools/agent-recorder/README.md`
- Project contribution guidelines: `CONTRIBUTING.md`
