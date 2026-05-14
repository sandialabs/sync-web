import { Fragment, useEffect, useState } from "react";
import type { FormEvent } from "react";
import { Configuration, FrontendApi, SettingsFlow } from "@ory/client-fetch";
import { Card, InputField, Node, NodeMessages } from "@ory/elements";
import AuthLayout from "../AuthLayout";

const kratos = new FrontendApi(
  new Configuration({ basePath: "/auth/.ory" })
);

export default function Settings() {
  const [flow, setFlow] = useState<SettingsFlow | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirmPassword, setConfirmPassword] = useState("");
  const [passwordMismatch, setPasswordMismatch] = useState(false);

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
        </div>
      </Card>
    </AuthLayout>
  );
}
