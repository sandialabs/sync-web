import { useEffect, useState } from "react";
import { Configuration, FrontendApi, LoginFlow } from "@ory/client-fetch";
import { UserAuthCard } from "@ory/elements";
import AuthLayout from "../AuthLayout";

const kratos = new FrontendApi(
  new Configuration({ basePath: "/auth/.ory" })
);

export default function Login() {
  const [flow, setFlow] = useState<LoginFlow | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    const flowId = params.get("flow");
    const returnTo = params.get("return_to");
    const returnToParam = returnTo ? `?return_to=${encodeURIComponent(returnTo)}` : "";
    if (!flowId) {
      window.location.replace(`/auth/.ory/self-service/login/browser${returnToParam}`);
      return;
    }
    kratos.getLoginFlow({ id: flowId }).then(setFlow).catch(() => {
      setError("Login session expired. Redirecting…");
      setTimeout(() => window.location.replace(`/auth/.ory/self-service/login/browser${returnToParam}`), 1500);
    });
  }, []);

  if (error) {
    return (
      <AuthLayout active="login" title="Sign in">
        <p className="auth-error">{error}</p>
      </AuthLayout>
    );
  }
  if (!flow) return null;

  const returnTo = flow.return_to;
  const signupURL = returnTo
    ? `/auth/registration?return_to=${encodeURIComponent(returnTo)}`
    : "/auth/registration";

  return (
    <AuthLayout active="login" title="Sign in">
      <UserAuthCard
        flowType="login"
        flow={flow}
        additionalProps={{
          signupURL,
        }}
      />
    </AuthLayout>
  );
}
