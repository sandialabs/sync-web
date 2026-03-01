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
- `SYNC_SERVICES_DIR` (default `/code/sync-services`, used by `capture:screenshots:stack`)
- `PORT`, `SECRET`, `PERIOD`, `WINDOW`, `LOCAL_LISP_PATH` (used by `capture:screenshots:stack`)
