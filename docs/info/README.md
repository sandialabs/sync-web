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

1. Bring up the compose stack so `/explorer/` and `/workbench/` are live (default expected URL is `http://127.0.0.1:8192`).
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
- `SYNC_REPO_ROOT` (required for `capture:screenshots:stack`): absolute path to the sync-web repo root
- `PORT`, `SECRET`, `PERIOD`, `WINDOW` (used by `capture:screenshots:stack`)

Example:

```bash
SYNC_REPO_ROOT=/absolute/path/to/sync-web npm run capture:screenshots:stack
```

## Multi-Node Testing

For local multi-node journal plus social-agent testing, use the compose harness in `tests/network/compose`:

```bash
cd /absolute/path/to/sync-web/tests/network/compose
python3 generate.py
docker compose up
```

That harness reuses the full general stack per node and is the current local path for testing bridge topology behavior without FIREWHEEL.
