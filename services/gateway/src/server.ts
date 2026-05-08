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
.swagger-ui .topbar { background-color: #101318; border-bottom: 1px solid #2c3440; }
.swagger-ui .topbar .topbar-wrapper img { max-height: 36px; width: auto; }
.swagger-ui .topbar .topbar-wrapper a.link {
  display: inline-flex;
  align-items: center;
  gap: 0.6rem;
}
.swagger-ui .topbar .topbar-wrapper a.link::after {
  content: "Synchronic Web Gateway";
  color: #ffffff;
  font-weight: 700;
  font-size: 1rem;
  letter-spacing: 0.01em;
  white-space: nowrap;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
}
.swagger-ui a { color: #0076a9; }
.swagger-ui .info .title { color: #1f2937; }
.swagger-ui .opblock.opblock-get { border-color: #0076a9; background: #f2f9fc; }
.swagger-ui .opblock.opblock-get .opblock-summary-method { background: #0076a9; }
.swagger-ui .opblock.opblock-post { border-color: #008e74; background: #f1fbf8; }
.swagger-ui .opblock.opblock-post .opblock-summary-method { background: #008e74; }
.swagger-ui .opblock-tag { color: #002b4c; }
.swagger-ui .scheme-container { box-shadow: none; }

/* Keep toolbar/search usable and clean */
.swagger-ui .topbar .download-url-wrapper { display: flex; align-items: center; gap: 0.5rem; }
.swagger-ui .topbar .download-url-wrapper input[type=text] { min-width: 18rem; }

/* Dark-mode friendly docs surface */
@media (prefers-color-scheme: dark) {
  .swagger-ui { color: #d4d4d4; }
  .swagger-ui .info .title,
  .swagger-ui .opblock-tag,
  .swagger-ui .info p,
  .swagger-ui .info li,
  .swagger-ui .info h1,
  .swagger-ui .info h2,
  .swagger-ui .info h3,
  .swagger-ui .info h4 { color: #d4d4d4; }
  .swagger-ui .scheme-container,
  .swagger-ui .opblock-description-wrapper,
  .swagger-ui .opblock-body,
  .swagger-ui .responses-inner h4,
  .swagger-ui .responses-inner h5 { background: #1f252b; color: #d4d4d4; }
  .swagger-ui .topbar { background-color: #101318; border-bottom-color: #2c3440; }
  .swagger-ui .opblock.opblock-get { background: #163041; border-color: #0076a9; }
  .swagger-ui .opblock.opblock-post { background: #16382f; border-color: #008e74; }
  .swagger-ui table thead tr td,
  .swagger-ui table thead tr th,
  .swagger-ui .parameter__name,
  .swagger-ui .parameter__type,
  .swagger-ui .response-col_status,
  .swagger-ui .response-col_description { color: #d4d4d4; }
}
`;

const swaggerUiAuthJs = `
(function () {
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

  function injectBadge(session) {
    const topbar = document.querySelector('.swagger-ui .topbar-wrapper');
    if (!topbar) return false;

    let badge = document.getElementById('sync-auth-badge');
    if (!badge) {
      badge = document.createElement('div');
      badge.id = 'sync-auth-badge';
      badge.style.cssText = [
        'display:flex', 'align-items:center', 'gap:0.6rem',
        'margin-left:auto', 'padding-right:1rem',
        'font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,Helvetica,Arial,sans-serif',
        'font-size:0.85rem', 'white-space:nowrap',
      ].join(';');
      topbar.appendChild(badge);
    }

    while (badge.firstChild) badge.removeChild(badge.firstChild);

    if (session.loggedIn) {
      if (session.email) {
        const emailSpan = document.createElement('span');
        emailSpan.style.color = '#e0f2fe';
        emailSpan.textContent = session.email;
        badge.appendChild(emailSpan);
      }
      const btn = document.createElement('button');
      btn.style.cssText = 'background:transparent;border:1px solid #00add0;color:#00add0;padding:0.2rem 0.65rem;border-radius:4px;cursor:pointer;font-size:0.85rem;';
      btn.textContent = 'Log out';
      btn.addEventListener('click', logout);
      badge.appendChild(btn);
    } else {
      const a = document.createElement('a');
      a.href = '/auth/.ory/self-service/login/browser?return_to=' + encodeURIComponent(window.location.href);
      a.style.cssText = 'background:#00add0;color:#fff;text-decoration:none;padding:0.25rem 0.75rem;border-radius:4px;font-weight:600;';
      a.textContent = 'Log in';
      badge.appendChild(a);
    }
    return true;
  }

  async function init() {
    let attempts = 0;
    const interval = setInterval(async () => {
      if (document.querySelector('.swagger-ui .topbar-wrapper') || attempts++ > 100) {
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
