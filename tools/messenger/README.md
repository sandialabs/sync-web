# Synchronic Messenger

Synchronic Messenger is an experimental, dependency-free web client for direct and small-group messaging over Sync Web. It runs as a standalone nginx sidecar and uses an existing Gateway and Kratos browser session; it adds no Records or Interface methods.

## Capabilities

- Direct v1 messages and minimal v2 group messages with fixed participants
- Explicit replies with local quoted-parent context
- Active and idle mailbox polling with strict envelope and path validation
- Durable contact registry with exact conditional updates
- Exact reconciliation of incoming mailbox authorization rules
- Local conversation projection, drafts, unread state, blocking, and themes
- Public contact profiles and exact conditional local profile editing
- Installable PWA shell and open-application browser notifications

The default contact registry is empty. Messenger never creates bridges, outbound routes, remote grants, recipient configuration, or trust relationships. Every correspondent must already be reachable through operator-configured Sync Web federation.

## Run

Messenger requires Docker Compose or a compatible Compose frontend and an existing Sync Web Gateway with Kratos authentication.

```sh
cp .env.example .env
docker compose up --build
```

Open exactly `http://localhost:8280`, or the configured `MESSENGER_PORT`. Do not use `127.0.0.1` in the browser: Kratos cookies are hostname-scoped, and ports do not create separate cookie domains.

Configuration:

- `SYNC_GATEWAY_ORIGIN`: nginx's upstream Gateway origin, normally `http://127.0.0.1:18192`
- `SYNC_GATEWAY_BROWSER_ORIGIN`: the browser-visible Gateway origin, normally `http://localhost:18192`
- `MESSENGER_PORT`: loopback listener port, default `8280`
- `MESSENGER_POLL_ACTIVE_MS`: visible-page polling interval, default `3000`
- `MESSENGER_POLL_IDLE_MS`: idle-page polling interval, default `15000`

Compose uses host networking so Messenger can reach a Gateway bound to host loopback. Nginx listens only on `127.0.0.1:${MESSENGER_PORT}`. Keep Messenger on localhost or an encrypted tunnel; public hosting belongs behind an HTTPS ingress.

## Contacts And Readiness

On first authenticated startup, `public/contacts.json` initializes the durable registry at:

```text
(*state* <username> messenger contacts.json)
```

The checked-in registry is intentionally empty. Contacts can be added in the UI. `examples/contacts.example.json` documents the seed format for controlled deployments that need a preconfigured registry.

For each contact, Messenger requires:

- a stable local contact ID and display handle;
- the remote endpoint identity, Journal, and owner;
- an existing outbound federation route;
- the exact receiver-relative incoming mailbox principal.

Messenger reconciles the signed-in user's incoming mailbox grants to the contact registry and requires exact readback before enabling send or poll. Removing a contact revokes its corresponding Messenger mailbox authority. Blocking is local-only and does not change durable authorization.

Successful exchange additionally depends on the remote owner's independent grant for the sender and the remote poller. Denied, ambiguous, or partial operations remain visible and are never automatically retried.

## Authentication And Secrets

Messenger uses the existing Kratos browser session. It obtains the authenticated username from `/auth/.ory/sessions/whoami` and the local Journal name from `/api/v1/general/info`.

There is no Messenger password, API token, Journal credential, Root secret, or Interface credential. Nginx strips browser-supplied `Authorization` headers and proxies only the Gateway operations needed for messaging, profiles, contact authorization, and change events. Gateway remains the sole custodian of the Interface credential.

## Local State

Conversation history is a bounded browser-local projection, partitioned by Messenger origin, authenticated username, and local Journal. It is not canonical Journal history and should not be used as a sensitive long-term archive. Contact configuration and profiles are durable Sync Web values updated through exact expected-value checks.

See [Architecture](docs/architecture.md) for message and profile boundaries and [Limitations](docs/limitations.md) for deliberately deferred functionality.

## Development

No package installation is required when Node.js is available:

```sh
node --check public/app.js
node --check public/api.mjs
node --check public/logic.mjs
node --check public/profile.mjs
node --check public/contact-profile.mjs
node --test tests/*.test.mjs
```

Or run the equivalent package script:

```sh
npm test
npm run check
```

## Layout

- `public/`: dependency-free browser application and PWA assets
- `deploy/`: bounded nginx runtime configuration
- `tests/`: codec, API, state, profile, and deployment tests
- `examples/`: inert example contact registry
- `Dockerfile`: static application and Gateway proxy image
- `compose.yaml`: standalone local sidecar
