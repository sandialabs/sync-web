# Run

Install dependencies:

```bash
pip install -r requirements.txt
```

Run locally with live reload:

```bash
mkdocs serve
```

Build static site:

```bash
mkdocs build
```

The generated site is available under `site/`.

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
- `SYNC_SCREENSHOT_DIR` (default `./docs/images/screenshots`)
- `SYNC_SCREENSHOT_SETTLE_MS` (default `1500`)
- `SYNC_SERVICES_DIR` (default `/code/sync-services`, used by `capture:screenshots:stack`)
- `PORT`, `SECRET`, `PERIOD`, `WINDOW`, `LOCAL_LISP_PATH` (used by `capture:screenshots:stack`)
