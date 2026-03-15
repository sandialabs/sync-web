# Run

Install dependencies:

```bash
npm install
```

Run locally with live reload:

```bash
npm run dev
```

Build static site:

```bash
npm run build
```

Preview static build:

```bash
npm run preview
```

The generated site is available under `dist/`.

## Content Location

- Active Starlight content lives in `src/content/docs`.
- Static assets (including screenshots) live in `public/images`.
- Legacy MkDocs files remain under `docs/` for reference only and are not used by the Astro build.

## UI Screenshot Refresh

Use this when you want to refresh Explorer/Workbench screenshots embedded in docs.

The current Explorer screenshot flow captures the default landing view, which now opens in `Ledger` mode.

Prerequisites:

1. Bring up `sync-services` so `/explorer/` and `/workbench/` are live (default expected URL is `http://127.0.0.1:8192`).
2. Install screenshot tooling:

```bash
npm install
npx playwright install chromium
```

Capture screenshots against an already running stack:

```bash
npm run capture:screenshots
```

Capture screenshots with a managed stack lifecycle (start, capture, stop):

```bash
npm run capture:screenshots:stack
```

Optional environment variables:

- `SYNC_BASE_URL` (default `http://127.0.0.1:8192`)
- `SYNC_SCREENSHOT_DIR` (default `./public/images/screenshots`)
- `SYNC_SCREENSHOT_SETTLE_MS` (default `1500`)
- `SYNC_SERVICES_DIR` (required for `capture:screenshots:stack`)
- `PORT`, `SECRET`, `PERIOD`, `WINDOW`, `LOCAL_LISP_PATH` (used by `capture:screenshots:stack`)

Example:

```bash
SYNC_SERVICES_DIR=/absolute/path/to/sync-services npm run capture:screenshots:stack
```

## Multi-Node Testing

For local multi-node journal plus social-agent testing, use the compose harness in `sync-analysis`:

```bash
cd /code/sync-analysis/compose/social-agent-network
SYNC_SERVICES_GENERAL_COMPOSE=/code/sync-services/compose/general/docker-compose.yml \
python3 generate.py
docker compose up
```

That harness reuses the full `sync-services` general stack per node and is the current local path for testing peer topology behavior without FIREWHEEL.
