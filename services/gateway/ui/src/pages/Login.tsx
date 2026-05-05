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
    const flowId = new URLSearchParams(window.location.search).get("flow");
    if (!flowId) {
      window.location.replace("/auth/.ory/self-service/login/browser");
      return;
    }
    kratos.getLoginFlow({ id: flowId }).then(setFlow).catch(() => {
      setError("Login session expired. Redirecting…");
      setTimeout(() => window.location.replace("/auth/.ory/self-service/login/browser"), 1500);
    });
  }, []);

  if (error) return <p>{error}</p>;
  if (!flow) return null;

  return (
    <UserAuthCard
      flowType="login"
      flow={flow}
      additionalProps={{
        forgotPasswordURL: "/auth/recovery",
        signupURL: "/auth/registration",
      }}
    />
  );
}
