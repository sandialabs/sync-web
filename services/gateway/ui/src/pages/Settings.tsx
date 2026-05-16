import { Fragment, useEffect, useState } from "react";
import type { FormEvent, KeyboardEvent } from "react";
import { Configuration, FrontendApi, SettingsFlow } from "@ory/client-fetch";
import { Card, InputField, Node, NodeMessages } from "@ory/elements";
import AuthLayout from "../AuthLayout";

const kratos = new FrontendApi(
  new Configuration({ basePath: "/auth/.ory" })
);

interface ApiTokenRow {
  id: string;
  description: string;
  created_at: string;
}

export default function Settings() {
  const [flow, setFlow] = useState<SettingsFlow | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirmPassword, setConfirmPassword] = useState("");
  const [passwordMismatch, setPasswordMismatch] = useState(false);

  const [tokens, setTokens] = useState<ApiTokenRow[]>([]);
  const [tokensLoading, setTokensLoading] = useState(true);
  const [tokensError, setTokensError] = useState<string | null>(null);
  const [newDescription, setNewDescription] = useState("");
  const [creating, setCreating] = useState(false);
  const [createError, setCreateError] = useState<string | null>(null);
  const [newToken, setNewToken] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [revoking, setRevoking] = useState<Set<string>>(new Set());

  useEffect(() => {
    const flowId = new URLSearchParams(window.location.search).get("flow");
    if (!flowId) {
      window.location.replace("/auth/.ory/self-service/settings/browser");
      return;
    }
    kratos.getSettingsFlow({ id: flowId }).then(setFlow).catch(() => {
      setError("Settings session expired. Redirecting…");
      setTimeout(() => window.location.replace("/auth/.ory/self-service/settings/browser"), 1500);
    });
  }, []);

  useEffect(() => {
    fetch("/api/v1/tokens")
      .then((res) => {
        if (!res.ok) throw new Error(`${res.status}`);
        return res.json() as Promise<ApiTokenRow[]>;
      })
      .then(setTokens)
      .catch(() => setTokensError("Failed to load API tokens."))
      .finally(() => setTokensLoading(false));
  }, []);

  const createToken = async () => {
    setCreating(true);
    setCreateError(null);
    try {
      const res = await fetch("/api/v1/tokens", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ description: newDescription.trim() }),
      });
      if (!res.ok) throw new Error(`${res.status}`);
      const created = await res.json() as { token: string; id: string; description: string; created_at: string };
      setNewToken(created.token);
      setNewDescription("");
      setTokens((prev) => [...prev, { id: created.id, description: created.description, created_at: created.created_at }]);
    } catch {
      setCreateError("Failed to create token. Try again.");
    } finally {
      setCreating(false);
    }
  };

  const revokeToken = async (id: string) => {
    setRevoking((prev) => new Set(prev).add(id));
    try {
      const res = await fetch(`/api/v1/tokens/${id}`, { method: "DELETE" });
      if (!res.ok) throw new Error(`${res.status}`);
      setTokens((prev) => prev.filter((token) => token.id !== id));
    } finally {
      setRevoking((prev) => {
        const next = new Set(prev);
        next.delete(id);
        return next;
      });
    }
  };

  const copyToken = () => {
    if (!newToken) return;
    navigator.clipboard.writeText(newToken).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  };

  const handleCreateTokenDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter" && newDescription.trim()) createToken();
  };

  if (error) {
    return (
      <AuthLayout active="settings" title="Account settings">
        <p className="auth-error">{error}</p>
      </AuthLayout>
    );
  }
  if (!flow) return null;

  const username =
    (flow.identity?.traits as { username?: string } | undefined)?.username ?? "Unavailable";
  const passwordNodes = flow.ui.nodes.filter((node) => {
    const attributes = node.attributes;
    if (!("name" in attributes)) return false;
    return node.group === "password" || attributes.name === "csrf_token";
  });
  const hasPasswordSettings = passwordNodes.some((node) => node.group === "password");

  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    const password = new FormData(event.currentTarget).get("password");
    if (typeof password === "string" && password !== confirmPassword) {
      event.preventDefault();
      setPasswordMismatch(true);
      return;
    }
    setPasswordMismatch(false);
  };

  const formatDate = (iso: string) => {
    try {
      return new Date(iso).toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });
    } catch {
      return iso;
    }
  };

  return (
    <AuthLayout active="settings" title="Account settings">
      <Card heading="Account">
        <div style={{ display: "grid", gap: "1.25rem" }}>
          <section style={{ display: "grid", gap: "0.75rem" }} aria-label="Profile">
            <InputField
              header="Username"
              value={username}
              disabled
              dataTestid="settings/username"
            />
            <p className="auth-muted">
              Usernames cannot be changed here. Contact your system administrator if
              this needs to be updated.
            </p>
          </section>

          <div className="auth-section-divider" />

          <section style={{ display: "grid", gap: "0.75rem" }} aria-label="Change password">
            <h2 className="auth-section-title">Change password</h2>
            {hasPasswordSettings ? (
              <form
                action={flow.ui.action}
                method={flow.ui.method}
                onSubmit={handleSubmit}
                style={{ display: "grid", gap: "1rem" }}
              >
                <NodeMessages uiMessages={flow.ui.messages} />
                {passwordNodes.map((node, index) => {
                  const attributes = node.attributes;
                  const name =
                    "name" in attributes && typeof attributes.name === "string"
                      ? attributes.name
                      : "";
                  return (
                    <Fragment key={`${name || node.group}-${index}`}>
                      <Node node={node} />
                      {name === "password" ? (
                        <InputField
                          header="Confirm password"
                          type="password"
                          autoComplete="new-password"
                          value={confirmPassword}
                          required
                          aria-invalid={passwordMismatch}
                          helperMessage={
                            passwordMismatch ? "Passwords do not match." : undefined
                          }
                          onChange={(event) => {
                            setConfirmPassword(event.currentTarget.value);
                            if (passwordMismatch) setPasswordMismatch(false);
                          }}
                          dataTestid="node/input/password_confirm"
                        />
                      ) : null}
                    </Fragment>
                  );
                })}
              </form>
            ) : (
              <p className="auth-error">Password changes are not available for this account.</p>
            )}
          </section>

          <div className="auth-section-divider" />

          <section style={{ display: "grid", gap: "0.75rem" }} aria-label="API tokens">
            <h2 className="auth-section-title">API tokens</h2>
            <p className="auth-muted">
              Use API tokens to authenticate headless callers (agents, scripts, CI) without
              a browser session. Pass the token as{" "}
              <code style={{ fontFamily: "ui-monospace, monospace", fontSize: "12px" }}>
                Authorization: Bearer &lt;token&gt;
              </code>
              .
            </p>

            {newToken && (
              <div style={{
                background: "var(--auth-bg-secondary)",
                border: "1px solid var(--auth-border)",
                borderRadius: "8px",
                padding: "12px 14px",
                display: "grid",
                gap: "8px",
              }}>
                <p style={{ margin: 0, fontWeight: 600, fontSize: "13px", color: "var(--auth-text-primary)" }}>
                  Copy your token now — it will not be shown again.
                </p>
                <div style={{ display: "flex", gap: "8px", alignItems: "flex-start", minWidth: 0 }}>
                  <code style={{
                    flex: 1,
                    minWidth: 0,
                    fontFamily: "ui-monospace, monospace",
                    fontSize: "11px",
                    lineHeight: 1.45,
                    background: "var(--auth-bg-primary)",
                    border: "1px solid var(--auth-border)",
                    borderRadius: "4px",
                    padding: "6px 8px",
                    overflowWrap: "normal",
                    wordBreak: "break-all",
                    whiteSpace: "normal",
                    display: "block",
                    color: "var(--auth-text-primary)",
                  }}>
                    {newToken}
                  </code>
                  <button
                    type="button"
                    onClick={copyToken}
                    style={{
                      flexShrink: 0,
                      padding: "6px 12px",
                      fontSize: "12px",
                      fontWeight: 600,
                      border: "1px solid var(--auth-border)",
                      borderRadius: "6px",
                      background: "var(--auth-bg-primary)",
                      color: "var(--auth-text-primary)",
                      cursor: "pointer",
                      whiteSpace: "nowrap",
                    }}
                  >
                    {copied ? "Copied" : "Copy"}
                  </button>
                </div>
                <button
                  type="button"
                  onClick={() => setNewToken(null)}
                  style={{
                    alignSelf: "start",
                    background: "none",
                    border: "none",
                    padding: 0,
                    fontSize: "12px",
                    color: "var(--auth-text-secondary)",
                    cursor: "pointer",
                    textDecoration: "underline",
                  }}
                >
                  Dismiss
                </button>
              </div>
            )}

            <div style={{ display: "flex", gap: "8px" }}>
              <input
                type="text"
                placeholder="Description (e.g. CI pipeline)"
                value={newDescription}
                onChange={(e) => setNewDescription(e.currentTarget.value)}
                onKeyDown={handleCreateTokenDown}
                disabled={creating}
                style={{
                  flex: 1,
                  padding: "8px 10px",
                  fontSize: "14px",
                  border: "1px solid var(--auth-border)",
                  borderRadius: "6px",
                  background: "var(--auth-bg-primary)",
                  color: "var(--auth-text-primary)",
                  outline: "none",
                  minWidth: 0,
                }}
              />
              <button
                type="button"
                onClick={createToken}
                disabled={creating || !newDescription.trim()}
                style={{
                  flexShrink: 0,
                  padding: "8px 14px",
                  fontSize: "13px",
                  fontWeight: 600,
                  border: "1px solid var(--auth-border)",
                  borderRadius: "6px",
                  background: "var(--auth-bg-secondary)",
                  color: "var(--auth-text-primary)",
                  cursor: creating || !newDescription.trim() ? "not-allowed" : "pointer",
                  opacity: creating || !newDescription.trim() ? 0.5 : 1,
                  whiteSpace: "nowrap",
                }}
              >
                {creating ? "Creating…" : "Create token"}
              </button>
            </div>
            {createError && <p className="auth-muted" style={{ color: "var(--auth-text-primary)" }}>{createError}</p>}

            {tokensLoading ? (
              <p className="auth-muted">Loading…</p>
            ) : tokensError ? (
              <p className="auth-muted">{tokensError}</p>
            ) : tokens.length === 0 ? (
              <p className="auth-muted">No API tokens yet.</p>
            ) : (
              <div style={{ display: "grid", gap: "6px" }}>
                {tokens.map((token) => (
                  <div
                    key={token.id}
                    style={{
                      display: "flex",
                      alignItems: "center",
                      gap: "10px",
                      padding: "8px 10px",
                      border: "1px solid var(--auth-border)",
                      borderRadius: "6px",
                      background: "var(--auth-bg-secondary)",
                    }}
                  >
                    <div style={{ flex: 1, minWidth: 0 }}>
                      <div style={{
                        fontSize: "13px",
                        fontWeight: 600,
                        color: "var(--auth-text-primary)",
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                        whiteSpace: "nowrap",
                      }}>
                        {token.description || <span style={{ color: "var(--auth-text-secondary)", fontWeight: 400 }}>No description</span>}
                      </div>
                      <div style={{ fontSize: "11px", color: "var(--auth-text-secondary)", marginTop: "2px" }}>
                        Created {formatDate(token.created_at)}
                      </div>
                    </div>
                    <button
                      type="button"
                      onClick={() => revokeToken(token.id)}
                      disabled={revoking.has(token.id)}
                      style={{
                        flexShrink: 0,
                        padding: "4px 10px",
                        fontSize: "12px",
                        fontWeight: 600,
                        border: "1px solid var(--auth-border)",
                        borderRadius: "6px",
                        background: "transparent",
                        color: "var(--auth-text-secondary)",
                        cursor: revoking.has(token.id) ? "not-allowed" : "pointer",
                        opacity: revoking.has(token.id) ? 0.5 : 1,
                      }}
                    >
                      {revoking.has(token.id) ? "Revoking…" : "Revoke"}
                    </button>
                  </div>
                ))}
              </div>
            )}
          </section>
        </div>
      </Card>
    </AuthLayout>
  );
}
