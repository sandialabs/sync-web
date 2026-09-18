# Messenger Limitations

Synchronic Messenger is experimental. Its current scope deliberately excludes:

- arbitrary contact discovery or onboarding;
- automatic profile discovery or background profile refresh;
- bridge, route, remote grant, or recipient configuration;
- agent inbox-context activation;
- closed-application Web Push and a subscription backend;
- sender-visible remote delivery receipts;
- create-only UUID publication and a durable prepared outbox;
- durable server-side conversation archive or search;
- automatic retry of denied, ambiguous, or partial operations.

The browser conversation projection is local convenience state. Clearing browser storage removes it, and another browser does not inherit it. Durable contacts and profiles remain in Sync Web.

The current Compose path uses host networking to share an existing localhost-scoped Kratos session and reach a loopback Gateway. That path is intended for Linux development and operator-controlled local use. A public deployment requires an HTTPS ingress and a separately reviewed cookie, origin, routing, and notification design.

Current federation proves the configured route and owner observation but does not establish malicious-intermediary-resistant terminal-Journal continuity for profile display. Messenger labels that boundary rather than inferring stronger identity.
