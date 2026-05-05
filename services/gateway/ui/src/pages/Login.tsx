import { useEffect, useState } from "react";
import { Configuration, FrontendApi, LoginFlow } from "@ory/client-fetch";
import { UserAuthCard } from "@ory/elements";

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

  if (error) return <p>{error}</p>;
  if (!flow) return null;

  const returnTo = flow.return_to;
  const signupURL = returnTo
    ? `/auth/registration?return_to=${encodeURIComponent(returnTo)}`
    : "/auth/registration";

  return (
    <UserAuthCard
      flowType="login"
      flow={flow}
      additionalProps={{
        forgotPasswordURL: "/auth/recovery",
        signupURL,
      }}
    />
  );
}
