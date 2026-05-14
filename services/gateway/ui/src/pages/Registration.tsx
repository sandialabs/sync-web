import { Fragment, useEffect, useState } from "react";
import type { FormEvent } from "react";
import { Configuration, FrontendApi, RegistrationFlow } from "@ory/client-fetch";
import { ButtonLink, Card, InputField, Node, NodeMessages } from "@ory/elements";

const kratos = new FrontendApi(
  new Configuration({ basePath: "/auth/.ory" })
);

export default function Registration() {
  const [flow, setFlow] = useState<RegistrationFlow | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirmPassword, setConfirmPassword] = useState("");
  const [passwordMismatch, setPasswordMismatch] = useState(false);

  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    const flowId = params.get("flow");
    const returnTo = params.get("return_to");
    const returnToParam = returnTo ? `?return_to=${encodeURIComponent(returnTo)}` : "";
    if (!flowId) {
      window.location.replace(`/auth/.ory/self-service/registration/browser${returnToParam}`);
      return;
    }
    kratos.getRegistrationFlow({ id: flowId }).then(setFlow).catch(() => {
      setError("Registration session expired. Redirecting…");
      setTimeout(() => window.location.replace(`/auth/.ory/self-service/registration/browser${returnToParam}`), 1500);
    });
  }, []);

  if (error) return <p>{error}</p>;
  if (!flow) return null;

  const returnTo = flow.return_to;
  const loginURL = returnTo
    ? `/auth/login?return_to=${encodeURIComponent(returnTo)}`
    : "/auth/login";

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
    <Card heading="Create account">
      <form
        action={flow.ui.action}
        method={flow.ui.method}
        onSubmit={handleSubmit}
        style={{ display: "grid", gap: "1rem" }}
      >
        <NodeMessages uiMessages={flow.ui.messages} />
        {flow.ui.nodes.map((node, index) => {
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
      <div style={{ marginTop: "1rem" }}>
        <span>Already have an account? </span>
        <ButtonLink href={loginURL}>Log in</ButtonLink>
      </div>
    </Card>
  );
}
