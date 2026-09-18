window.MESSENGER_CONFIG = Object.freeze({
  gatewayBase: "/api/v1",
  authBase: "/auth",
  loginUrl: "http://localhost:18192/auth/login",
  logoutUrl: "http://localhost:18192/auth/.ory/self-service/logout/browser",
  pollActiveMs: 3000,
  pollIdleMs: 15000,
  maxMessageBytes: 65536
});
