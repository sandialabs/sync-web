# Manual Messaging Demo

A local HTTP, Linux amd64 demonstration of human/agent and agent/agent Messages.
One Compose file starts eight services, without profiles:

- Human `journal`, `gateway`, `identity-provider`, and `router`.
- `explorer` and a locally built `messenger`.
- Locally built `agent-1` and `agent-2`, each running Pi, a private Ledger,
  repository `journal-cli`, and a small resident inbox extension.

This is a manual demo, not fleet provisioning or a production deployment.
Bridges and receiver-local mailbox grants remain explicit operator/agent actions.
Configured contacts and recipients do not establish trust or readiness.

## Start

From this directory, with Docker Compose and Python 3.11+ available:

```sh
cp .env.example .env
# Edit the four LITELLM_* settings in .env.
docker compose build
docker compose up -d
docker compose ps
```

Open **http://localhost:8280/** for Messenger and **http://localhost:8192/explorer/**
for Explorer. The normal router/Gateway stack uses `DEMO_PORT` (8192); Messenger
has its own `MESSENGER_PORT` (8280). Use `localhost`, not `127.0.0.1`,
for browser login: the identity provider's cookies are hostname-scoped.
Sign in as `admin`; the disposable default password is `demo-password`.
Locally built agents and Messenger have explicit `localhost/...` image tags and are never pulled from a remote registry; build them before starting. Base images have fully qualified registry names, avoiding Podman short-name selection prompts. There are no TLS certificates, ACME directories, HTTPS ports, or profiles.
The router binds only to host loopback. No host Pi directory or container socket
is mounted into an agent.

On Linux, `host.docker.internal` maps to the host gateway. A host LiteLLM server
must listen on an address reachable from the container network, not only its own
loopback. Alternatively use an externally reachable LiteLLM URL. Never commit
`.env` or paste model tokens into Pi prompts.

`podman compose` with a compatible Compose provider can be used instead. The
agent application runs as the image's unprivileged Node user. Builds use a pinned
Pi version and released amd64 Ledger/kernel artifacts verified against release
checksums; they do not compile Rust, LLVM, or Wasmer. The human Journal uses the
same platform release image. Native ARM and macOS execution are out of scope.

## Configuration

| Setting | Default / meaning |
| --- | --- |
| `LITELLM_URL` | Provider base URL, including `/v1` when required |
| `LITELLM_TOKEN` | Provider credential, supplied only at runtime |
| `LITELLM_MODEL` | Model identifier configured by your proxy |
| `LITELLM_API` | `openai-completions`; use a Pi-supported API such as `openai-responses` or `anthropic-messages` to match the proxy |
| `DEMO_PORT` | `8192`, normal router/Gateway/Explorer HTTP port |
| `MESSENGER_PORT` | `8280`, standalone Messenger HTTP port |
| `JOURNAL_PERIOD` | `8` seconds, for all three Journals |
| `POLL_ACTIVE_MS` | `5000` |
| `POLL_RECENT_MS` | `15000` |
| `POLL_IDLE_MS` | `30000` |
| `POLL_RECENT_FOR_MS` | `300000` |
| `POLL_JITTER` | `0.15` |
| `MESSENGER_POLL_ACTIVE_MS` | `3000` |
| `MESSENGER_POLL_IDLE_MS` | `15000` |

Both agents use the same configurable provider settings. Model metadata uses a
conservative 32k context/4096 output, text-only, non-reasoning configuration;
these are demo defaults, not an assertion of the upstream model's capabilities.
The token is referenced from the environment rather than written into models.json
or passed in command arguments. Pi's terminal UI is the foreground application;
the lightweight launcher supervises Pi and the local Ledger together.

For a responsive presentation, put these overrides in `.env` before starting:

```dotenv
JOURNAL_PERIOD=1
POLL_ACTIVE_MS=1000
POLL_RECENT_MS=1000
POLL_IDLE_MS=1000
POLL_JITTER=0
MESSENGER_POLL_ACTIVE_MS=1000
MESSENGER_POLL_IDLE_MS=1000
```

Shorter intervals mean higher frequency. Polling alone cannot eliminate Journal
commit, bridge synchronization, or model-response latency. An accepted write is
not proof of receipt.

## Attach And Warm Up

Use two terminal windows, keeping each Pi process already running in its container:

```sh
docker attach "$(docker compose ps -q agent-1)"
docker attach "$(docker compose ps -q agent-2)"
```

Detach with **Ctrl-P, Ctrl-Q**; do not start a second Pi using `compose exec pi`.
Ctrl-D exits Pi and stops that agent container. `docker compose start agent-1`
starts it again. Pi resumes its existing session; canonical inbox entries provide
restart deduplication. The poller does not delete or acknowledge mailbox bytes.
With Podman, use `podman attach` and `podman compose ps -q` instead.

Each agent has local instructions and its CLI configuration already installed.
Ask it to inspect `journal-cli --help` and run:

```sh
journal-cli journal request info
journal-cli peer signing-key-digest
```

The human CLI wrapper uses this checkout's Python CLI and the same demo
Interface credential as Gateway. If you customized `.env`, load it into your
shell before using the helper (review the file before sourcing it):

```sh
set -a; . ./.env; set +a
python3 human.py peer signing-key-digest
```

Only public signing-key digests need to be exchanged. Do not copy Root secrets,
Interface credentials, or LiteLLM tokens into messages or prompts.

1. Get both agents' public signing-key SHA-256 Base64 values from their terminals.
2. Preapprove each agent on the human hub, replacing the placeholders:

```sh
python3 human.py peer preapprove agent-1 --signing-key-sha256-base64 '<agent-1 digest>'
python3 human.py peer preapprove agent-2 --signing-key-sha256-base64 '<agent-2 digest>'
```

3. Give each agent the human hub's public digest. Ask `agent-1` to perform:

```sh
journal-cli peer preapprove human --signing-key-sha256-base64 '<human digest>'
journal-cli peer bridge human http://router/api/v1/journal/interface --remote-name agent-1
journal-cli peer authorize human admin mailbox/inbox/human/admin
journal-cli peer authorize human/agent-2 agent-2 mailbox/inbox/agent-2/agent-2
```

For `agent-2`, use `--remote-name agent-2` and change the other-agent grant to
`human/agent-1 agent-1 mailbox/inbox/agent-1/agent-1`.
Bridge creation establishes the reciprocal hub bridge; do not blindly repeat
failed or completion-unknown bridge requests. Inspect the current state instead.

4. Verify both local `human` routes, then `human/agent-2` from agent-1 and
   `human/agent-1` from agent-2. Read-only route checks can be repeated while
   commits synchronize. Their proof-bearing output can be large.
5. Log into Messenger. The two seeded contacts cause its ordinary explicit
   contact reconciliation to establish the human's incoming mailbox grants.
   Contacts do not create bridges. Check that send readiness is established.

These grants allow only puts and read-only use at the exact receiver-local
mailbox subtree, with key-index `(0 -1)` and no run/retrieve authority.
The owner principal is `(human *state* admin)`, not an extra admin hop.
The other agent's incoming principal includes the hub and the other-agent alias.

## Demonstrate

- Send agent-1 a request in Messenger. Observe the incoming Message in its Pi
  session and ask it to reply using the CLI, not just terminal text.
- Ask agent-1 to send agent-2 a request via the configured `human/agent-2` route.
  Observe agent-2's session and its reply in agent-1's session.
- Optionally create a Messenger group containing both agents; direct v1 and
  group v2 envelopes are validated using Messenger's existing codec.
- Use Explorer to inspect the human mailbox and compare Message IDs/reply
  references. Look at actual receipt, not only the sender's `write-accepted`.

The Python producer emits canonical millisecond timestamps for both direct and
independent group copies. No Message protocol or server interpretation changes.
A real LiteLLM/model-assisted presentation remains a manual acceptance step.

## Stop And Reset

```sh
docker compose down
```

This retains all four named volumes: human Journal, identity provider, and both
agent homes (including their Journals, private keys/config and Pi sessions).
Keep demo secrets stable across restarts; changing environment values does not
perform credential rotation in an existing Journal.

For an explicitly destructive clean-slate reset, `docker compose down -v`
removes **all** demo histories, keys, users and agent sessions. Re-warm bridges
and grants afterward. Never use this command against a production project.

## Cheap Checks

```sh
# PyYAML is the existing topology-test dependency.
python3 -m pip install -r ../../network/compose/requirements.txt
python3 -m unittest discover -s . -p 'test_*.py'
node --test agent/inbox.test.mjs
PYTHONPATH=../../../tools/journal-cli python3 -m unittest discover -s ../../../tools/journal-cli/tests
# Use config --quiet with actual credentials; full config output can expose env values.
docker compose config --quiet
```
