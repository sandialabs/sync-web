import Fastify from "fastify";
import fastifySwagger from "@fastify/swagger";
import fastifySwaggerUi from "@fastify/swagger-ui";
import fastifyStatic from "@fastify/static";
import { existsSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { getConfig } from "./config";
import { createJournalClient } from "./journal";
import { createKratosClient } from "./kratos";
import { instrumentGatewayRequests } from "./metrics";
import { gatewayRoutes } from "./routes";

const apiDescription = `
Versioned, function-oriented HTTP gateway over Synchronic journal transport endpoints.

Start here:
- Use GET routes for simple read-only checks: /api/v1/general/size and /api/v1/general/info.
- Use POST /api/v1/general/{operation} for function calls that take arguments.
- Use POST /api/v1/general/batch for ordered multi-request workflows under one authenticated call.
- For restricted routes, provide either Authorization bearer token or X-Sync-Auth header.

Request bodies:
- JSON mode uses application/json with keyword arguments as a direct object body.
- Scheme mode uses text/plain or application/scheme with a raw Scheme arguments expression.

Operational notes:
- General routes are intended for normal app integrations.
- Root routes are admin-level and only exposed when ALLOW_ADMIN_ROUTES=1.
`.trim();

const swaggerUiTheme = `
.swagger-ui { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif; }
.swagger-ui .topbar {
  display: none;
}
.sync-doc-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  background-color: #101318;
  border-bottom: 1px solid #2c3440;
  color: #f3f6fb;
  min-height: 58px;
  padding: 10px 18px;
  box-sizing: border-box;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
}
.sync-doc-toolbar-left,
.sync-doc-toolbar-right {
  display: flex;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
}
.sync-doc-logo {
  width: 36px;
  height: 36px;
  object-fit: contain;
}
.sync-doc-tabs {
  display: flex;
  align-items: center;
  gap: 8px;
}
.sync-doc-tab {
  padding: 7px 15px;
  border: 1px solid rgba(255, 255, 255, 0.18);
  border-radius: 999px;
  color: #f3f6fb;
  background: transparent;
  font-size: 13px;
  font-weight: 600;
  line-height: 1;
  letter-spacing: 0;
  white-space: nowrap;
  text-decoration: none;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
}
.sync-doc-tab:hover {
  background: rgba(255, 255, 255, 0.1);
  text-decoration: none;
}
.sync-doc-tab.active {
  background: rgba(255, 255, 255, 0.14);
  border-color: rgba(255, 255, 255, 0.3);
}
.sync-doc-auth {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 4px 8px 4px 12px;
  border: 1px solid rgba(255, 255, 255, 0.18);
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.06);
  font-size: 0.85rem;
  white-space: nowrap;
}
.sync-doc-auth.logged-out {
  padding: 0;
  border: none;
  background: transparent;
}
.sync-doc-auth-name {
  color: #f3f6fb;
  opacity: 0.85;
  max-width: 200px;
  overflow: hidden;
  text-overflow: ellipsis;
}
.sync-doc-auth-button {
  border-radius: 999px;
  cursor: pointer;
  font-weight: 600;
  text-decoration: none;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
}
.sync-doc-auth-login {
  background: transparent;
  border: 1px solid rgba(255, 255, 255, 0.18);
  color: #f3f6fb;
  padding: 7px 15px;
  font-size: 13px;
}
.sync-doc-auth-logout {
  background: transparent;
  border: 1px solid rgba(255, 255, 255, 0.25);
  color: #f3f6fb;
  padding: 3px 10px;
  font-size: 12px;
  opacity: 0.8;
}
.sync-doc-auth-logout:hover {
  opacity: 1;
  background: rgba(255, 255, 255, 0.1);
}
.sync-doc-theme-toggle {
  width: 36px;
  height: 36px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border: 1px solid rgba(255, 255, 255, 0.18);
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.08);
  color: #f3f6fb;
  cursor: pointer;
  font-size: 16px;
  line-height: 1;
}
.sync-doc-theme-toggle:hover {
  background: rgba(255, 255, 255, 0.14);
}
html[data-theme="dark"] body {
  background: #181a1f;
}
html[data-theme="dark"] .swagger-ui {
  color: #d4d4d4;
}
html[data-theme="dark"] .swagger-ui .info .title,
html[data-theme="dark"] .swagger-ui .opblock-tag,
html[data-theme="dark"] .swagger-ui .info p,
html[data-theme="dark"] .swagger-ui .info li,
html[data-theme="dark"] .swagger-ui .info h1,
html[data-theme="dark"] .swagger-ui .info h2,
html[data-theme="dark"] .swagger-ui .info h3,
html[data-theme="dark"] .swagger-ui .info h4 {
  color: #d4d4d4;
}
html[data-theme="dark"] .swagger-ui .scheme-container,
html[data-theme="dark"] .swagger-ui .opblock-description-wrapper,
html[data-theme="dark"] .swagger-ui .opblock-body,
html[data-theme="dark"] .swagger-ui .responses-inner h4,
html[data-theme="dark"] .swagger-ui .responses-inner h5 {
  background: #1f252b;
  color: #d4d4d4;
}
html[data-theme="dark"] .swagger-ui .opblock.opblock-get {
  background: #163041;
  border-color: #0076a9;
}
html[data-theme="dark"] .swagger-ui .opblock.opblock-post {
  background: #16382f;
  border-color: #008e74;
}
html[data-theme="dark"] .swagger-ui table thead tr td,
html[data-theme="dark"] .swagger-ui table thead tr th,
html[data-theme="dark"] .swagger-ui .parameter__name,
html[data-theme="dark"] .swagger-ui .parameter__type,
html[data-theme="dark"] .swagger-ui .response-col_status,
html[data-theme="dark"] .swagger-ui .response-col_description {
  color: #d4d4d4;
}
.swagger-ui a { color: #0076a9; }
.swagger-ui .info .title { color: #1f2937; }
.swagger-ui .opblock.opblock-get { border-color: #0076a9; background: #f2f9fc; }
.swagger-ui .opblock.opblock-get .opblock-summary-method { background: #0076a9; }
.swagger-ui .opblock.opblock-post { border-color: #008e74; background: #f1fbf8; }
.swagger-ui .opblock.opblock-post .opblock-summary-method { background: #008e74; }
.swagger-ui .opblock-tag { color: #002b4c; }
.swagger-ui .scheme-container { box-shadow: none; }

@media (max-width: 640px) {
  .sync-doc-toolbar {
    align-items: flex-start;
    flex-direction: column;
  }
  .sync-doc-toolbar-right {
    width: 100%;
  }
  .sync-doc-tabs {
    flex-wrap: wrap;
  }
}

/* Default to OS dark preference until the user explicitly toggles the docs page. */
@media (prefers-color-scheme: dark) {
  html:not([data-theme="light"]) body { background: #181a1f; }
  html:not([data-theme="light"]) .swagger-ui { color: #d4d4d4; }
  html:not([data-theme="light"]) .swagger-ui .info .title,
  html:not([data-theme="light"]) .swagger-ui .opblock-tag,
  html:not([data-theme="light"]) .swagger-ui .info p,
  html:not([data-theme="light"]) .swagger-ui .info li,
  html:not([data-theme="light"]) .swagger-ui .info h1,
  html:not([data-theme="light"]) .swagger-ui .info h2,
  html:not([data-theme="light"]) .swagger-ui .info h3,
  html:not([data-theme="light"]) .swagger-ui .info h4 { color: #d4d4d4; }
  html:not([data-theme="light"]) .swagger-ui .scheme-container,
  html:not([data-theme="light"]) .swagger-ui .opblock-description-wrapper,
  html:not([data-theme="light"]) .swagger-ui .opblock-body,
  html:not([data-theme="light"]) .swagger-ui .responses-inner h4,
  html:not([data-theme="light"]) .swagger-ui .responses-inner h5 { background: #1f252b; color: #d4d4d4; }
  html:not([data-theme="light"]) .swagger-ui .opblock.opblock-get { background: #163041; border-color: #0076a9; }
  html:not([data-theme="light"]) .swagger-ui .opblock.opblock-post { background: #16382f; border-color: #008e74; }
  html:not([data-theme="light"]) .swagger-ui table thead tr td,
  html:not([data-theme="light"]) .swagger-ui table thead tr th,
  html:not([data-theme="light"]) .swagger-ui .parameter__name,
  html:not([data-theme="light"]) .swagger-ui .parameter__type,
  html:not([data-theme="light"]) .swagger-ui .response-col_status,
  html:not([data-theme="light"]) .swagger-ui .response-col_description { color: #d4d4d4; }
}
`;

const swaggerUiAuthJs = `
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
    const btn = document.getElementById('sync-doc-theme-toggle');
    if (!btn) return;
    const nextTheme = theme === 'light' ? 'dark' : 'light';
    btn.textContent = theme === 'light' ? '◐' : '◑';
    btn.title = 'Switch to ' + nextTheme + ' mode';
    btn.setAttribute('aria-label', 'Switch to ' + nextTheme + ' mode');
  }

  function wireThemeToggle() {
    applyTheme(getPreferredTheme());
    const btn = document.getElementById('sync-doc-theme-toggle');
    if (!btn || btn.dataset.bound === 'true') return;
    btn.dataset.bound = 'true';
    btn.addEventListener('click', function () {
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
        return { loggedIn: true, email: data?.identity?.traits?.username ?? data?.identity?.traits?.email ?? '' };
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

  function ensureToolbar() {
    let toolbar = document.getElementById('sync-doc-toolbar');
    if (toolbar) return toolbar;

    const swaggerRoot = document.getElementById('swagger-ui');
    if (!swaggerRoot || !swaggerRoot.parentNode) return null;

    toolbar = document.createElement('div');
    toolbar.id = 'sync-doc-toolbar';
    toolbar.className = 'sync-doc-toolbar';
    toolbar.innerHTML =
      '<div class="sync-doc-toolbar-left">' +
        '<img class="sync-doc-logo" src="/gateway-logo.png" alt="Synchronic Web" />' +
        '<nav class="sync-doc-tabs" aria-label="Gateway sections">' +
          '<a class="sync-doc-tab" href="/gateway">Gateway Home</a>' +
          '<span class="sync-doc-tab active">API Reference</span>' +
        '</nav>' +
      '</div>' +
      '<div class="sync-doc-toolbar-right">' +
        '<div id="sync-auth-badge" class="sync-doc-auth"><span>Checking session...</span></div>' +
        '<button id="sync-doc-theme-toggle" class="sync-doc-theme-toggle" type="button" title="Switch to dark mode" aria-label="Switch to dark mode">◐</button>' +
      '</div>';
    swaggerRoot.parentNode.insertBefore(toolbar, swaggerRoot);
    wireThemeToggle();
    return toolbar;
  }

  function injectBadge(session) {
    const toolbar = ensureToolbar();
    if (!toolbar) return false;
    const badge = document.getElementById('sync-auth-badge');
    if (!badge) return false;

    badge.className = 'sync-doc-auth';
    while (badge.firstChild) badge.removeChild(badge.firstChild);

    if (session.loggedIn) {
      if (session.email) {
        const emailSpan = document.createElement('span');
        emailSpan.className = 'sync-doc-auth-name';
        emailSpan.textContent = session.email;
        badge.appendChild(emailSpan);
      }
      const btn = document.createElement('button');
      btn.className = 'sync-doc-auth-button sync-doc-auth-logout';
      btn.textContent = 'Sign out';
      btn.addEventListener('click', logout);
      badge.appendChild(btn);
    } else {
      badge.classList.add('logged-out');
      const a = document.createElement('a');
      a.href = '/auth/.ory/self-service/login/browser?return_to=' + encodeURIComponent(window.location.href);
      a.className = 'sync-doc-auth-button sync-doc-auth-login';
      a.textContent = 'Log in';
      badge.appendChild(a);
    }
    return true;
  }

  async function init() {
    let attempts = 0;
    const interval = setInterval(async () => {
      if (document.getElementById('swagger-ui') || attempts++ > 100) {
        clearInterval(interval);
        const session = await getSession();
        injectBadge(session);
      }
    }, 100);
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', init);
  } else {
    init();
  }
})();
`;

const readGatewayLogo = (): Buffer | null => {
  try {
    return readFileSync(resolve(__dirname, "..", "assets", "sync-web-logo.png"));
  } catch {
    return null;
  }
};


const main = async (): Promise<void> => {
  const config = getConfig();
  const app = Fastify({
    logger: true,
    bodyLimit: 64 * 1024 * 1024,
    ajv: { customOptions: { keywords: ["example"], allowUnionTypes: true } },
  });
  instrumentGatewayRequests(app);
  const logo = readGatewayLogo();
  if (logo) {
    app.get("/gateway-logo.png", { schema: { hide: true } }, async (_request, reply) =>
      reply.type("image/png").send(logo)
    );
  }

  app.addContentTypeParser("text/plain", { parseAs: "string" }, (_req, body, done) =>
    done(null, body)
  );
  app.addContentTypeParser("application/scheme", { parseAs: "string" }, (_req, body, done) =>
    done(null, body)
  );
  app.addContentTypeParser("application/x-www-form-urlencoded", { parseAs: "string" }, (_req, body, done) =>
    done(null, body)
  );
  // Default Content-Type to text/plain when the header is absent so that sync-remote's
  // Scheme bodies (sent without a Content-Type) reach the text/plain parser instead of
  // triggering a 415. Runs before body parsing; all other content types are unaffected.
  app.addHook("preParsing", async (request, _reply, payload) => {
    if (!request.headers["content-type"]) {
      request.headers["content-type"] = "text/plain";
    }
    return payload;
  });

  await app.register(fastifySwagger, {
    openapi: {
      info: {
        title: "Synchronic Gateway API",
        description: apiDescription,
        version: "1.0.0",
      },
      servers: [{ url: "/" }],
      tags: [
        {
          name: "Health",
          description: "Process and upstream readiness endpoints.",
        },
        {
          name: "General API (Public)",
          description: "Public general operations that do not require auth.",
        },
        {
          name: "General API (Restricted)",
          description:
            "Authenticated general operations for reads/writes, bridge config, and stepping.",
        },
        {
          name: "Root API (Admin)",
          description:
            "High-privilege root-plane operations for runtime management and updates.",
        },
        {
          name: "Journal (Proxy)",
          description:
            "Thin pass-through endpoints that forward directly to the journal interface without transformation.",
        },
      ],
    },
  });

  await app.register(fastifySwaggerUi, {
    routePrefix: "/api/v1/docs",
    staticCSP: true,
    ...(logo
      ? {
          logo: {
            type: "image/png",
            content: logo,
            href: "/",
            target: "_self",
          },
        }
      : {}),
    theme: {
      css: [{ filename: "sync-gateway-swagger-theme.css", content: swaggerUiTheme }],
      js: [{ filename: "sync-gateway-auth.js", content: swaggerUiAuthJs }],
    },
    uiConfig: {
      docExpansion: "list",
      deepLinking: true,
displayRequestDuration: true,
    },
  });

  const journal = createJournalClient(
    config.journalJsonEndpoint,
    config.journalSchemeEndpoint,
    config.rootJsonEndpoint,
    config.rootSchemeEndpoint,
    config.requestTimeoutMs,
    app.log,
    {
      debugForwarding: config.debugForwarding,
      debugForwardingIncludeAuth: config.debugForwardingIncludeAuth,
    }
  );

  // Auth UI: serve static files from ui/dist, with SPA fallback for client-side routes.
  // The /auth/.ory/* proxy must be registered before the catch-all below.
  await app.register(fastifyStatic, {
    root: config.authUiDir,
    prefix: "/auth/",
    wildcard: false,
    decorateReply: true,
  });

  // Kratos public API proxy: strips /auth/.ory prefix and forwards to Kratos.
  // Handles browser redirects (redirect: manual), forwards cookies both ways.
  app.route({
    method: ["GET", "POST", "PUT", "DELETE"],
    url: "/auth/.ory/*",
    schema: { hide: true },
    handler: async (request, reply) => {
      const upstreamPath = request.url.replace(/^\/auth\/.ory/, "");
      const url = `${config.kratosPublicUrl}${upstreamPath}`;

      const headers: Record<string, string> = {
        accept: (request.headers.accept as string) ?? "application/json",
      };
      if (request.headers.cookie) headers["cookie"] = request.headers.cookie as string;
      if (request.headers["content-type"]) headers["content-type"] = request.headers["content-type"] as string;
      if (request.headers["x-session-token"]) headers["x-session-token"] = request.headers["x-session-token"] as string;

      let body: string | undefined;
      if (request.method !== "GET" && request.method !== "HEAD" && request.body != null) {
        body = typeof request.body === "string" ? request.body : JSON.stringify(request.body);
        if (!headers["content-type"]) headers["content-type"] = "application/json";
      }

      const response = await fetch(url, {
        method: request.method,
        headers,
        body,
        redirect: "manual",
      });

      reply.code(response.status);

      const contentType = response.headers.get("content-type");
      if (contentType) reply.header("content-type", contentType);

      const location = response.headers.get("location");
      if (location) reply.header("location", location);

      const setCookies = response.headers.getSetCookie?.() ?? [];
      for (const cookie of setCookies) reply.header("set-cookie", cookie);

      const text = await response.text();
      return reply.send(text || null);
    },
  });

  // SPA fallback: serve index.html for any /auth/* path not matched by a static file
  // or the .ory proxy above. Enables client-side routing in the auth UI.
  app.get("/auth/*", async (request, reply) => {
    const pathname = request.url.split("?")[0];
    const relPath = pathname.replace(/^\/auth\/?/, "") || "index.html";
    const filePath = resolve(config.authUiDir, relPath);
    if (filePath.startsWith(config.authUiDir) && existsSync(filePath)) {
      return reply.sendFile(relPath);
    }
    return reply.sendFile("index.html");
  });

  const kratos = createKratosClient(config.kratosPublicUrl);

  await app.register(gatewayRoutes, {
    journal,
    allowAdminRoutes: config.allowAdminRoutes,
    journalSecret: config.journalSecret,
    kratos,
  });

  await app.listen({
    host: config.host,
    port: config.port,
  });


};

main().catch((error) => {
  // eslint-disable-next-line no-console
  console.error(error);
  process.exit(1);
});
