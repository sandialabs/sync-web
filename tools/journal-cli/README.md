# journal-cli

`journal-cli` is the single agent-invoked Python CLI for Sync Web 1.6 Journal and related owner workflows. It talks to an already-running Ledger through `/interface`; it does not start the Ledger, own its database, poll an inbox, wake an agent, or include Synchronic Messenger.

The executable is one public entry point with modular internal code:

```text
journal-cli journal ...
journal-cli message ...
journal-cli peer ...
journal-cli profile ...
journal-cli source ...
journal-cli flow ...
journal-cli config ...
journal-cli doctor
```

The tool is Sync Web 1.6-only. It emits no compatibility requests using `get`, `set!`, `get-batch`, `set-batch!`, `call!`, or `resolve`.

## Install

```sh
python3 -m pip install ./tools/journal-cli
journal-cli --version
```

For repository development without installation:

```sh
PYTHONPATH=tools/journal-cli tools/journal-cli/journal-cli --help
```

## Configuration

By default the CLI reads `/etc/pi-agent/agent.json`. It accepts the installed fixed-agent configuration or this standalone shape:

```json
{
  "version": 1,
  "owner": "alice",
  "journal": "alice",
  "endpoint": "http://127.0.0.1:8192/interface",
  "credentialFile": "/private/alice.interface-secret",
  "stateDir": "/private/sync-state",
  "inboxConfig": "/private/sync-inbox.json",
  "timeoutSeconds": 30
}
```

Credential values are read from private files and are never accepted as command-line arguments. Endpoints must be verified HTTPS or literal-loopback HTTP. Redirects, environment proxies, and automatic mutation retries are disabled.

Use `--config PATH` before the command group:

```sh
journal-cli --config ./agent.json config validate
journal-cli --config ./agent.json doctor
```

## Journal operations

`journal request` accepts a current 1.6 operation and a JSON arguments object. The JSON may be inline, read from `@FILE`, or read from standard input with `-`.

```sh
journal-cli journal request info
journal-cli journal request use! \
  --arguments-json '{"path":["*state*","alice","notes"],"read-only?":true,"expression?":false}'
journal-cli journal request retrieve \
  --arguments-json '{"path":[-1,"*state*","alice","notes"],"pinned?":false,"proof?":false}'
journal-cli journal request put! --arguments-json @put.json
```

Use `--route first-hop/terminal` for a signed federated application request. Raw Scheme is explicit and file/stdin-only:

```sh
journal-cli journal raw request.scm
journal-cli journal raw - < request.scm
```

Every ordinary command emits a `journal-cli-outcome-v1` JSON line. Exit status distinguishes pre-dispatch failure (`2`), explicit rejection (`3`), and completion not established (`4`). Never retry status `4` automatically.

## Message commands

Message commands are one-shot operations over the accepted Message envelope and mailbox paths. They do not replace or include the resident inbox polling/wake service.

```sh
journal-cli message status
journal-cli message send bob --body 'hello'
journal-cli message send bob@bob --body-file ./message.txt --in-reply-to UUID
journal-cli message group --to bob --to carol --conversation-id UUID --body 'hello all'
journal-cli message read bob@bob MESSAGE_UUID
```

Group writes have independent per-recipient outcomes. Failed or ambiguous recipients are not retried.

## Peer and profile commands

```sh
journal-cli peer capability-card descriptor.json --json
journal-cli peer registry list
journal-cli peer registry validate
journal-cli peer route galactica/bob --explain-identity alice
journal-cli peer preapprove galactica --id-base64 BASE64_ID
journal-cli peer bridge galactica https://example.test/interface --remote-name alice
journal-cli peer delete-bridge galactica --expected-public-key-sha256 SHA256
journal-cli peer authorize galactica/bob bob mailbox/inbox/alice/alice --owner alice
journal-cli peer authorizations --owner alice --digest
journal-cli peer recipient-route-replace --journal bob --identity bob --owner bob \
  --from-route old/bob --to-route new/bob --expect-config-digest SHA256
journal-cli peer mailbox-doctor doctor bob@bob --route galactica/bob --json
journal-cli peer enrollment plan ...

journal-cli profile validate ...
journal-cli profile fetch ...
journal-cli profile publish-create ...
journal-cli profile wait-commit ...
```

Enrollment retains plan-before-apply behavior. It may manage the separately installed resident inbox service when explicitly invoked with `enroll`; that service is not part of this package.

## Source Publication

Source Publication and producer/reviewer workflows are bundled as internal modules; no second adapter executable is required. Source v2 is a clean Sync Web 1.6 contract: the canonical Interface endpoint is the global publisher locator, while route names and fixed hop history indexes preserve observer-relative continuity. It does not parse Source v1 Journal-identity references. See [`docs/source-v2.md`](docs/source-v2.md).

Profile v2 similarly replaces the removed Journal identity field with the canonical publisher Interface endpoint. Consumer entry endpoints and routes remain observer-relative provenance. Profile v0/v1 is not accepted or migrated. See [`docs/profile-v2.md`](docs/profile-v2.md).

```sh
journal-cli source ops get-current REQUEST.json
journal-cli source publication inspect --reference fixed-reference.scm --route galactica/publisher
journal-cli source publication publish ...
journal-cli source publication ready ...
journal-cli source publication pull ...
journal-cli source publication audit-current ...
journal-cli source publication recover ...

journal-cli flow producer ...
journal-cli flow ready ...
journal-cli flow reviewer ...
journal-cli flow diagnose ...
journal-cli flow validate-package ...
```

Source receipts, exact CAS behavior, chunks, manifests, fixed references, ready markers, materialization, audit, and recovery remain application-level workflows. A successful low-level write is not treated as publication completion.

## Development

Run the complete standalone suite:

```sh
cd tools/journal-cli
PYTHONDONTWRITEBYTECODE=1 PYTHONPATH=. python3 -m unittest discover -s tests -v
```

The suite includes the imported Source Publication, Source flow, profile, capability-card, mailbox diagnostic, and bounded low-level adapter contracts plus 1.6 request and Message tests. Test fixtures may contain legacy names as negative or migration evidence; live request construction may not.

This standalone tool has its own version. Changes here do not bump the platform `VERSION` or Journal SDK crate version unless they also modify those products.
