import { resolve } from "node:path";

export interface GatewayConfig {
  host: string;
  port: number;
  journalEndpoint: string;
  rootEndpoint: string;
  requestTimeoutMs: number;
  allowAdminRoutes: boolean;
  debugForwarding: boolean;
  debugForwardingIncludeAuth: boolean;
  kratosPublicUrl: string;
  kratosAdminUrl: string;
  authUiDir: string;
  journalSecret: string;
}

const toNumber = (value: string | undefined, fallback: number): number => {
  if (!value) return fallback;
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : fallback;
};

const toBoolean = (value: string | undefined, fallback: boolean): boolean => {
  if (!value) return fallback;
  const normalized = value.trim().toLowerCase();
  if (["1", "true", "yes", "on"].includes(normalized)) return true;
  if (["0", "false", "no", "off"].includes(normalized)) return false;
  return fallback;
};

export const getConfig = (): GatewayConfig => ({
  host: process.env.HOST || "0.0.0.0",
  port: toNumber(process.env.PORT, 8180),
  journalEndpoint:
    process.env.JOURNAL_ENDPOINT || "http://127.0.0.1:8192/interface",
  rootEndpoint:
    process.env.ROOT_ENDPOINT || "http://127.0.0.1:8192/interface",
  requestTimeoutMs: toNumber(process.env.REQUEST_TIMEOUT_MS, 30000),
  allowAdminRoutes: toBoolean(process.env.ALLOW_ADMIN_ROUTES, false),
  debugForwarding: toBoolean(process.env.DEBUG_FORWARDING, false),
  debugForwardingIncludeAuth: toBoolean(
    process.env.DEBUG_FORWARDING_INCLUDE_AUTH,
    false
  ),
  kratosPublicUrl:
    process.env.KRATOS_PUBLIC_URL || "http://identity-provider:4433",
  kratosAdminUrl:
    process.env.KRATOS_ADMIN_URL || "http://identity-provider:4434",
  authUiDir:
    process.env.AUTH_UI_DIR || resolve(__dirname, "../ui/dist"),
  journalSecret: process.env.JOURNAL_SECRET || "",
});
