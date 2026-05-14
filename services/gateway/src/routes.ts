import type { FastifyPluginAsync, FastifyRequest } from "fastify";
import { resolveIdentity, UnauthorizedError } from "./auth";
import type { KratosClient } from "./kratos";
import type { JournalClient } from "./journal";
import { JournalSemanticError } from "./journal";

export interface GatewayRoutesOptions {
  journal: JournalClient;
  allowAdminRoutes: boolean;
  journalSecret: string;
  kratos: KratosClient;
}

const getContentType = (request: FastifyRequest): string =>
  String(request.headers["content-type"] || "")
    .split(";")[0]
    .trim()
    .toLowerCase();

const isSchemeContentType = (contentType: string): boolean =>
  contentType === "text/plain" || contentType === "application/scheme";

const isJsonContentType = (contentType: string): boolean =>
  contentType === "application/json";

const escapeLispString = (value: string): string =>
  value.replace(/\\/g, "\\\\").replace(/"/g, '\\"');

const extractJsonArguments = (body: unknown): unknown => {
  if (body === undefined) {
    return undefined;
  }
  if (Array.isArray(body)) {
    return body;
  }
  if (!body || typeof body !== "object") {
    throw new Error(
      "JSON body must provide an argument object/array."
    );
  }
  const record = body as Record<string, unknown>;

  if ("arguments" in record) {
    throw new Error(
      "Gateway JSON bodies must provide operation arguments directly, not under an arguments wrapper."
    );
  }

  if ("function" in record || "authentication" in record) {
    throw new Error(
      "Gateway JSON bodies should provide only operation arguments."
    );
  }

  // Treat plain object bodies as direct keyword argument objects.
  return record;
};

const extractSchemeArguments = (body: unknown): string => {
  if (typeof body === "string") return body;
  if (Buffer.isBuffer(body)) return body.toString("utf8");
  throw new Error("Scheme requests must provide plain text argument expression body");
};

const buildSchemeExpression = (
  functionName: string,
  argsExpression: string,
  authSecret?: string,
  identityId?: string
): string => {
  const parts = [`(function ${functionName})`, `(arguments ${argsExpression})`];
  if (authSecret) {
    if (identityId) {
      parts.push(`(authentication ((identity ("${escapeLispString(identityId)}")) (credentials ("${escapeLispString(authSecret)}" #f))))`);
    } else {
      parts.push(`(authentication ((identity ()) (credentials ("${escapeLispString(authSecret)}"))))`);
    }
  }
  return `(${parts.join(" ")})`;
};

const buildRootSchemeExpression = (
  functionName: string,
  argsExpression: string,
  authSecret: string
): string => {
  const trimmed = argsExpression.trim();
  if (trimmed === "" || trimmed === "()") {
    return `(${functionName} "${escapeLispString(authSecret)}")`;
  }
  return `(${functionName} "${escapeLispString(authSecret)}" ${trimmed})`;
};

const callWithNegotiation = async (input: {
  request: FastifyRequest;
  journal: JournalClient;
  functionName: string;
  requiresAuth: boolean;
  root?: boolean;
  journalSecret: string;
  kratos: KratosClient;
}): Promise<unknown> => {
  const { request, journal, functionName, requiresAuth, root = false, journalSecret, kratos } = input;
  const resolved = requiresAuth
    ? await resolveIdentity(request, journalSecret, kratos)
    : undefined;
  const authSecret = resolved?.journalSecret;
  const identityId = resolved?.identityId;
  const contentType = getContentType(request);

  if (isSchemeContentType(contentType)) {
    const argsExpression = extractSchemeArguments(request.body);
    const expression =
      root && authSecret
        ? buildRootSchemeExpression(functionName, argsExpression, authSecret)
        : buildSchemeExpression(functionName, argsExpression, authSecret, identityId);
    return root
      ? journal.callRootScheme({ expression, functionName })
      : journal.callScheme({ expression, functionName });
  }

  if (!isJsonContentType(contentType)) {
    throw new Error(
      "Unsupported content-type. Use application/json or text/plain (or application/scheme)."
    );
  }

  const args = extractJsonArguments(request.body);
  return root
    ? journal.callRootJson({
        functionName,
        args,
        authentication: authSecret,
      })
    : journal.callJson({
        functionName,
        args,
        authentication: authSecret,
        identityId,
      });
};

const generalAliases = {
  get: "get",
  set: "set!",
  pin: "pin!",
  unpin: "unpin!",
  batch: "batch!",
  info: "info",
  synchronize: "synchronize",
  resolve: "resolve",
  trace: "trace",
  bridge: "bridge!",
  config: "config",
  "set-secret": "*secret*",
} as const;

const rootAliases = {
  eval: "*eval*",
  call: "*call*",
  step: "*step*",
  "set-secret": "*set-secret*",
  "set-step": "*set-step*",
  "set-query": "*set-query*",
} as const;

const publicGeneralFunctions = new Set<string>(["synchronize", "trace"]);
const requestModeDescription =
  "JSON mode: Content-Type application/json with a keyword argument object. Legacy array arguments are also accepted for compatibility. Scheme mode: Content-Type text/plain or application/scheme with a raw Scheme arguments expression (the gateway wraps it into the full journal transport call).";

const generalOperationDocs: Record<string, { summary: string; description: string }> = {
  get: {
    summary: "Read staged state",
    description:
      "Calls general function `get`. Reads the current staged view only.",
  },
  set: {
    summary: "Stage a state write",
    description:
      "Calls general function `set!`. Writes to staged state; pair with root `step` for durable chain progression.",
  },
  pin: {
    summary: "Pin state/proof into permanent history",
    description:
      "Calls general function `pin!`. Keeps selected path/proof material across retention windows.",
  },
  unpin: {
    summary: "Remove a previously pinned path/proof",
    description:
      "Calls general function `unpin!`. Returns selected content to normal retention behavior.",
  },
  batch: {
    summary: "Execute multiple general requests in order",
    description:
      "Calls general function `batch!`. Accepts a `queries` list of request-shaped entries and executes them in order against the ledger.",
  },
  info: {
    summary: "Get public info",
    description:
      "Calls public general function `info`. Returns public node metadata.",
  },
  synchronize: {
    summary: "Generate synchronization payload",
    description:
      "Calls public general function `synchronize`. Used by bridges/services to fetch digest/proof material for anti-entropy synchronization.",
  },
  resolve: {
    summary: "Resolve committed chain content",
    description:
      "Calls general function `resolve`. Reads indexed/committed content with optional pinned/proof metadata.",
  },
  trace: {
    summary: "Trace remote content against a chain index",
    description:
      "Calls public general function `trace`. Used by bridges/services to fetch a serialized remote path view from a committed chain index.",
  },
  bridge: {
    summary: "Register or update a bridge",
    description:
      "Calls general function `bridge!` with a bridge name and interface URL so the journal can wire its standard info/synchronize/trace handlers.",
  },
  config: {
    summary: "Read full node config",
    description:
      "Calls general function `config`. Includes private/runtime fields and should be treated as sensitive output.",
  },
  "set-secret": {
    summary: "Rotate the general interface secret",
    description:
      "Calls general function `*secret*`. Updates the shared interface secret used for restricted general operations.",
  },
};

const rootOperationDocs: Record<string, { summary: string; description: string }> = {
  eval: {
    summary: "Evaluate Scheme in admin context",
    description:
      "Calls root function `*eval*`. Highly privileged and intended for tightly controlled operations only.",
  },
  call: {
    summary: "Invoke function against root object",
    description:
      "Calls root function `*call*`. Supports runtime-level updates and administrative transformations.",
  },
  step: {
    summary: "Execute full root step cycle",
    description:
      "Calls root function `*step*`. Triggers configured step handler pipeline.",
  },
  "set-secret": {
    summary: "Rotate admin/root secret",
    description:
      "Calls root function `*set-secret*`. Changes the root credential.",
  },
  "set-step": {
    summary: "Replace step handler",
    description:
      "Calls root function `*set-step*`. Updates the root-plane step function at runtime.",
  },
  "set-query": {
    summary: "Replace query handler",
    description:
      "Calls root function `*set-query*`. Updates the root-plane query function at runtime.",
  },
};

const makeBodyContent = (jsonExample?: unknown, schemeExample?: string) => ({
  content: {
    "application/json": {
      schema: {
        type: ["array", "object"],
        description: "Keyword argument object (preferred) or legacy array.",
        ...(jsonExample !== undefined ? { example: jsonExample } : {}),
      },
    },
    "text/plain": {
      schema: {
        type: "string",
        description: "Raw Scheme arguments expression.",
        ...(schemeExample !== undefined ? { example: schemeExample } : {}),
      },
    },
    "application/scheme": {
      schema: {
        type: "string",
        description: "Raw Scheme arguments expression.",
        ...(schemeExample !== undefined ? { example: schemeExample } : {}),
      },
    },
  },
});

const generalOperationExamples: Record<string, unknown> = {
  get:          { path: [["*state*", "mykey"]] },
  set:          { path: [["*state*", "mykey"]], value: "myvalue" },
  pin:          { path: [-1, ["*state*", "mykey"]] },
  unpin:        { path: [-1, ["*state*", "mykey"]] },
  resolve:      { path: [-1, ["*state*", "mykey"]], "pinned?": true, "proof?": false },
  batch:        { queries: [{ function: "get", arguments: { path: [["*state*", "mykey"]] } }, { function: "config" }] },
  info:         {},
  bridge:       { name: "peer-a", interface: "http://peer-a/interface" },
  config:       {},
  "set-secret": { secret: "new-secret" },
  synchronize:  { index: 0 },
  trace:        { index: 0, path: [-1, ["*state*", "mykey"]] },
};

const generalSchemeExamples: Record<string, string> = {
  get:          "((path ((*state* mykey))))",
  set:          "((path ((*state* mykey))) (value myvalue))",
  pin:          "((path (-1 (*state* mykey))))",
  unpin:        "((path (-1 (*state* mykey))))",
  resolve:      "((path (-1 (*state* mykey))) (pinned? #t) (proof? #f))",
  batch:        "((queries (((function get) (arguments ((path ((*state* mykey))))) ((function config))))))",
  info:         "()",
  bridge:       "((name peer-a) (interface \"http://peer-a/interface\"))",
  config:       "()",
  "set-secret": "((secret new-secret))",
  synchronize:  "((index 0))",
  trace:        "((index 0) (path (-1 (*state* mykey))))",
};

const rootOperationExamples: Record<string, unknown> = {
  eval:          [["+", 1, 2]],
  call:          [["lambda", ["root"], [["root", { "*type/quoted*": "get" }], { "*type/quoted*": ["root", "object", "ledger"] }]]],
  step:          [],
  "set-secret":  [{ "*type/string*": "new-admin-secret" }],
  "set-step":    [["lambda", ["root", "secret", "query"], "root"]],
  "set-query":   [["lambda", ["root", "query"], "root"]],
};

const rootSchemeExamples: Record<string, string> = {
  eval:          "(+ 1 2)",
  call:          "(lambda (root) ((root 'get) '(root object ledger)))",
  step:          "",
  "set-secret":  '"new-admin-secret"',
  "set-step":    "(lambda (root secret query) root)",
  "set-query":   "(lambda (root query) root)",
};


export const gatewayRoutes: FastifyPluginAsync<GatewayRoutesOptions> = async (
  app,
  { journal, allowAdminRoutes, journalSecret, kratos }
) => {
  const rootRoutePath = "/api/v1/root";

  app.get("/", async (_request, reply) =>
    reply.type("text/html").send(`<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Synchronic Gateway</title>
    <style>
      :root {
        color-scheme: light;
        --blue: #00add0;
        --medium-blue: #0076a9;
        --dark-blue: #002b4c;
        --teal: #008e74;
        --blue-gray: #7d8ea0;
        --toolbar-bg: #171a1f;
        --toolbar-text: #ffffff;
        --bg-primary: #ffffff;
        --bg-secondary: #f8f8f8;
        --text-primary: #002b4c;
        --text-secondary: #7d8ea0;
        --border-color: #e0e0e0;
      }
      [data-theme="dark"] {
        color-scheme: dark;
        --toolbar-bg: #101318;
        --toolbar-text: #f3f6fb;
        --bg-primary: #181a1f;
        --bg-secondary: #20242b;
        --text-primary: #f0f3f8;
        --text-secondary: #a1abb8;
        --border-color: #343b46;
      }
      body {
        font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
        margin: 0;
        line-height: 1.45;
        background: var(--bg-primary);
        color: var(--text-primary);
      }
      code { font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; }
      main {
        max-width: 880px;
        padding: 2rem;
      }
      .toolbar {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 16px;
        padding: 10px 18px;
        min-height: 58px;
        box-sizing: border-box;
        background-color: var(--toolbar-bg);
        color: var(--toolbar-text);
      }
      .toolbar-left,
      .toolbar-right {
        display: flex;
        align-items: center;
        gap: 12px;
        flex-wrap: wrap;
      }
      .toolbar-logo {
        width: 36px;
        height: 36px;
        object-fit: contain;
      }
      .toolbar-nav {
        display: flex;
        align-items: center;
        gap: 8px;
      }
      .toolbar-pill {
        padding: 7px 15px;
        border: 1px solid rgba(255, 255, 255, 0.18);
        border-radius: 999px;
        background: transparent;
        color: var(--toolbar-text);
        font-size: 13px;
        font-weight: 600;
        line-height: 1;
      }
      .toolbar-pill.active {
        background: rgba(255, 255, 255, 0.14);
        border-color: rgba(255, 255, 255, 0.3);
      }
      .toolbar-pill:hover {
        background: rgba(255, 255, 255, 0.1);
        text-decoration: none;
      }
      .card {
        border: 1px solid var(--border-color);
        border-radius: 8px;
        padding: 1rem 1.2rem;
        margin: 1rem 0;
        background: var(--bg-secondary);
      }
      h1 { margin-top: 0; }
      ul { padding-left: 1.2rem; }
      a { color: var(--medium-blue); text-decoration: none; }
      a:hover { text-decoration: underline; }
      #auth-status {
        display: flex;
        align-items: center;
        gap: 8px;
        padding: 4px 8px 4px 12px;
        border: 1px solid rgba(255, 255, 255, 0.18);
        border-radius: 999px;
        font-size: 0.85rem;
        background: rgba(255, 255, 255, 0.06);
        color: var(--toolbar-text);
      }
      .auth-btn {
        padding: 3px 10px;
        border-radius: 999px;
        font-size: 12px;
        font-weight: 600;
        cursor: pointer;
        text-decoration: none;
      }
      .auth-name-link {
        color: var(--toolbar-text);
        opacity: 0.85;
        max-width: 200px;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        text-decoration: none;
      }
      .auth-name-link:hover {
        opacity: 1;
        text-decoration: underline;
      }
      .auth-btn-login {
        background: transparent;
        color: var(--toolbar-text);
        border: 1px solid rgba(255, 255, 255, 0.18);
        padding: 7px 15px;
        font-size: 13px;
      }
      #auth-status.logged-out {
        padding: 0;
        border: none;
        background: transparent;
      }
      .toolbar-icon {
        width: 36px;
        height: 36px;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        border: 1px solid rgba(255, 255, 255, 0.18);
        border-radius: 999px;
        background: rgba(255, 255, 255, 0.08);
        color: var(--toolbar-text);
        cursor: pointer;
        font-size: 16px;
        line-height: 1;
      }
      .toolbar-icon:hover {
        background: rgba(255, 255, 255, 0.14);
      }
      .auth-btn-logout {
        background: transparent;
        color: var(--toolbar-text);
        border: 1px solid rgba(255, 255, 255, 0.25);
        opacity: 0.8;
      }
      .auth-btn-logout:hover {
        opacity: 1;
        background: rgba(255, 255, 255, 0.1);
      }
      @media (max-width: 640px) {
        .toolbar {
          align-items: flex-start;
          flex-direction: column;
        }
        .toolbar-right {
          width: 100%;
        }
        .toolbar-nav {
          flex-wrap: wrap;
        }
        #auth-status {
          max-width: 100%;
        }
        main {
          padding: 1.25rem;
        }
      }
    </style>
  </head>
  <body>
    <div class="toolbar">
      <div class="toolbar-left">
        <img class="toolbar-logo" src="/gateway-logo.png" alt="Synchronic Web" />
        <nav class="toolbar-nav" aria-label="Gateway sections">
          <span class="toolbar-pill active">Gateway</span>
          <a class="toolbar-pill" href="/api/v1/docs">API Reference</a>
        </nav>
      </div>
      <div class="toolbar-right">
        <div id="auth-status">
          <span id="auth-label">Checking session…</span>
        </div>
        <button id="theme-toggle" class="toolbar-icon" type="button" title="Switch to dark mode" aria-label="Switch to dark mode">◐</button>
      </div>
    </div>

    <main>
      <h1>Synchronic Gateway</h1>

      <p>
        Web-facing gateway for Synchronic <code>general</code> and optional <code>root</code> operations.
        This service forwards operation calls to journal endpoints with session-based authentication.
      </p>
      <p>
        Use this service when you want stable, versioned HTTP endpoints that map directly to function-level journal calls
        while preserving authentication and request-shape consistency across clients.
      </p>

      <div class="card">
        <h2>API Docs</h2>
        <ul>
          <li><a href="/api/v1/docs">Swagger UI</a> (<code>/api/v1/docs</code>)</li>
        </ul>
        <p>
          Start there for route-by-route schemas, authentication requirements, and JSON/Scheme request-body guidance.
        </p>
      </div>

    <div class="card">
      <h2>Route Groups</h2>
      <ul>
        <li><code>/api/v1/general/*</code>: primary app-facing operations.</li>
        <li><code>/api/v1/root/*</code>: admin operations (only when enabled).</li>
        <li><code>/healthz</code> and <code>/readyz</code>: container and dependency probes.</li>
      </ul>
    </div>

    <div class="card">
      <h2>Common Patterns</h2>
      <ul>
        <li>Public reads: <code>GET /api/v1/general/size</code>, <code>GET /api/v1/general/info</code>.</li>
        <li>Restricted operations require a valid Kratos session cookie — <a href="/auth/login">log in</a> first.</li>
        <li>Mutating calls are <code>POST</code> and accept either JSON or Scheme argument bodies.</li>
      </ul>
    </div>

    <div class="card">
      <h2>Health</h2>
      <ul>
        <li><a href="/healthz"><code>/healthz</code></a></li>
        <li><a href="/readyz"><code>/readyz</code></a></li>
      </ul>
    </div>

    <div class="card">
      <h2>Quick Start</h2>
      <p>Public size call:</p>
      <pre><code>curl http://127.0.0.1:8180/api/v1/general/size</code></pre>
      <p>Authenticated call (pass session cookie from browser):</p>
      <pre><code>curl -X POST http://127.0.0.1:8180/api/v1/general/get \\
  -H "Cookie: ory_kratos_session=&lt;session&gt;" \\
  -H "Content-Type: application/json" \\
  -d '{"path":[["*state*","docs"]]}'</code></pre>
      <p>Scheme body call:</p>
      <pre><code>curl -X POST http://127.0.0.1:8180/api/v1/general/get \\
  -H "Cookie: ory_kratos_session=&lt;session&gt;" \\
  -H "Content-Type: text/plain" \\
  -d '((path ((*state* docs))))'</code></pre>
    </div>
    </main>
  </body>
  <script>
    (function () {
      const THEME_KEY = 'sync-gateway-theme';

      function getPreferredTheme() {
        const stored = localStorage.getItem(THEME_KEY);
        if (stored === 'light' || stored === 'dark') return stored;
        return window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches
          ? 'dark'
          : 'light';
      }

      function applyTheme(theme) {
        document.documentElement.setAttribute('data-theme', theme);
        const btn = document.getElementById('theme-toggle');
        if (!btn) return;
        const nextTheme = theme === 'light' ? 'dark' : 'light';
        btn.textContent = theme === 'light' ? '◐' : '◑';
        btn.title = 'Switch to ' + nextTheme + ' mode';
        btn.setAttribute('aria-label', 'Switch to ' + nextTheme + ' mode');
      }

      applyTheme(getPreferredTheme());
      const themeToggle = document.getElementById('theme-toggle');
      if (themeToggle) {
        themeToggle.addEventListener('click', function () {
          const current = document.documentElement.getAttribute('data-theme') === 'dark' ? 'dark' : 'light';
          const next = current === 'light' ? 'dark' : 'light';
          localStorage.setItem(THEME_KEY, next);
          applyTheme(next);
        });
      }

      async function getSession() {
        try {
          const res = await fetch('/auth/.ory/sessions/whoami', {
            credentials: 'include',
            headers: { accept: 'application/json' },
          });
          if (res.ok) {
            const data = await res.json();
            return { loggedIn: true, name: data?.identity?.traits?.username ?? '' };
          }
        } catch (_) {}
        return { loggedIn: false };
      }

      async function logout() {
        try {
          const res = await fetch('/auth/.ory/self-service/logout/browser?return_to=' + encodeURIComponent(window.location.origin + '/api/v1/docs'), {
            credentials: 'include',
            headers: { accept: 'application/json' },
          });
          if (res.ok) {
            const data = await res.json();
            if (data.logout_url) { window.location.href = data.logout_url; return; }
          }
        } catch (_) {}
        window.location.href = '/auth/login';
      }

      getSession().then(function (session) {
        const el = document.getElementById('auth-status');
        const label = document.getElementById('auth-label');
        if (!el || !label) return;
        if (session.loggedIn) {
          el.classList.add('logged-in');
          if (session.name) {
            const accountLink = document.createElement('a');
            accountLink.className = 'auth-name-link';
            accountLink.href = '/auth/settings';
            accountLink.title = 'Account settings';
            accountLink.textContent = session.name;
            label.replaceWith(accountLink);
          } else {
            label.textContent = 'Signed in';
          }
          const btn = document.createElement('button');
          btn.className = 'auth-btn auth-btn-logout';
          btn.textContent = 'Sign out';
          btn.addEventListener('click', logout);
          el.appendChild(btn);
        } else {
          el.classList.add('logged-out');
          label.remove();
          const a = document.createElement('a');
          a.className = 'auth-btn auth-btn-login';
          a.href = '/auth/.ory/self-service/login/browser?return_to=' + encodeURIComponent(window.location.origin + '/api/v1/docs');
          a.textContent = 'Log in';
          el.appendChild(a);
        }
      });
    })();
  </script>
</html>`)
  );

  app.get("/docs", async (_request, reply) => reply.redirect("/api/v1/docs"));
  app.get("/api/docs", async (_request, reply) => reply.redirect("/api/v1/docs"));

  app.get(
    "/healthz",
    {
      schema: {
        tags: ["Health"],
        summary: "Liveness probe",
        description:
          "Returns process liveness only. Does not verify journal connectivity.",
        response: {
          200: {
            type: "object",
            properties: { ok: { type: "boolean" } },
            required: ["ok"],
          },
        },
      },
    },
    async () => ({ ok: true })
  );

  app.get(
    "/readyz",
    {
      schema: {
        tags: ["Health"],
        summary: "Readiness probe",
        description:
          "Verifies the gateway can successfully execute a lightweight upstream call (`size`) against the journal JSON endpoint.",
        response: {
          200: {
            type: "object",
            properties: { ok: { type: "boolean" } },
            required: ["ok"],
          },
          503: {
            type: "object",
            properties: { ok: { type: "boolean" }, error: { type: "string" } },
            required: ["ok", "error"],
          },
        },
      },
    },
    async (_request, reply) => {
      try {
        await journal.callJson({ functionName: "size" });
        return { ok: true };
      } catch {
        return reply.code(503).send({ ok: false, error: "journal_unavailable" });
      }
    }
  );

  app.get(
    "/api/v1/general/size",
    {
      schema: {
        tags: ["General API (Public)"],
        summary: "Get ledger size (public)",
        description:
          "Public convenience endpoint for general function `size`. Useful for quick health/chain progression checks.",
      },
    },
    async () => journal.callJson({ functionName: "size" })
  );

  app.get(
    "/api/v1/general/info",
    {
      schema: {
        tags: ["General API (Public)"],
        summary: "Get public info (public)",
        description:
          "Public convenience endpoint for general function `info`. Returns public node metadata.",
      },
    },
    async () => journal.callJson({ functionName: "info" })
  );

  app.post(
    "/api/v1/journal/interface",
    {
      schema: {
        tags: ["Journal (Proxy)"],
        summary: "Transparent journal interface proxy",
        description:
          "Thin pass-through to the journal interface. Scheme bodies (text/plain or application/scheme) are forwarded as-is to the journal Scheme endpoint. JSON bodies are forwarded as-is to the journal JSON endpoint. No authentication injection or body transformation. Intended for journal-to-journal bridge calls.",
        body: makeBodyContent(
          { function: "size" },
          "((function size))"
        ),
      },
    },
    async (request, reply) => {
      const contentType = getContentType(request);
      if (isSchemeContentType(contentType)) {
        const expression = extractSchemeArguments(request.body);
        return journal.callScheme({ expression, functionName: "interface" });
      }
      if (isJsonContentType(contentType)) {
        return journal.proxyJson(request.body);
      }
      return reply.code(415).send({
        error: "unsupported_media_type",
        message: "Use application/json, text/plain, or application/scheme.",
      });
    }
  );

  for (const [operation, functionName] of Object.entries(generalAliases)) {
    const requiresAuth = !publicGeneralFunctions.has(functionName);
    app.post(
      `/api/v1/general/${operation}`,
      {
        schema: {
          tags: [
            requiresAuth ? "General API (Restricted)" : "General API (Public)",
          ],
          summary:
            generalOperationDocs[operation]?.summary ||
            `General operation '${operation}'`,
          description: `${generalOperationDocs[operation]?.description || "General operation."} ${requestModeDescription}`,
          body: makeBodyContent(generalOperationExamples[operation], generalSchemeExamples[operation]),
        },
      },
      async (request) =>
        callWithNegotiation({
          request,
          journal,
          functionName,
          requiresAuth,
          journalSecret,
          kratos,
        })
    );
  }

  if (allowAdminRoutes) {
    for (const [operation, functionName] of Object.entries(rootAliases)) {
      app.post(
        `${rootRoutePath}/${operation}`,
        {
          schema: {
            tags: ["Root API (Admin)"],
            summary:
              rootOperationDocs[operation]?.summary ||
              `Root operation '${operation}'`,
            description: `${rootOperationDocs[operation]?.description || "Root operation."} ${requestModeDescription}`,
            body: makeBodyContent(rootOperationExamples[operation], rootSchemeExamples[operation]),
          },
        },
        async (request) =>
          callWithNegotiation({
            request,
            journal,
            functionName,
            requiresAuth: true,
            root: true,
            journalSecret,
            kratos,
          })
      );
    }
  }

  app.setErrorHandler((error, request, reply) => {
    const asRecord =
      typeof error === "object" && error !== null
        ? (error as Record<string, unknown>)
        : {};
    const errorMessage =
      error instanceof Error ? error.message : String(asRecord.message || error);

    // Fastify validation errors should be surfaced as 400, not generic gateway failures.
    if ("validation" in asRecord && asRecord.validation) {
      return reply.code(400).send({
        error: "invalid_request",
        message: errorMessage,
      });
    }
    // Unsupported media types can be raised by Fastify before handler logic runs.
    if (asRecord.code === "FST_ERR_CTP_INVALID_MEDIA_TYPE") {
      return reply.code(415).send({
        error: "unsupported_media_type",
        message: errorMessage,
      });
    }
    if (error instanceof UnauthorizedError) {
      return reply.code(401).send({
        error: "unauthorized",
        message: "Valid Kratos session cookie required",
      });
    }
    if (errorMessage.includes("Unsupported content-type")) {
      return reply.code(415).send({
        error: "unsupported_media_type",
        message: errorMessage,
      });
    }
    if (
      errorMessage.includes("JSON body must provide") ||
      errorMessage.includes("JSON body must use") ||
      errorMessage.includes("Gateway JSON bodies must provide") ||
      errorMessage.includes("Gateway JSON bodies should provide") ||
      errorMessage.includes("Scheme requests must provide")
    ) {
      return reply.code(400).send({
        error: "invalid_request",
        message: errorMessage,
      });
    }
    if (error instanceof JournalSemanticError) {
      return reply.code(error.statusCode).send({
        error: error.code || "journal_error",
        message: error.message,
        details: error.details,
        source: "journal",
      });
    }
    request.log.error({ err: error }, "Unhandled gateway error");
    return reply.code(502).send({
      error: "gateway_error",
      message: errorMessage,
    });
  });
};
