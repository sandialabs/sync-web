import { useEffect, useState } from "react";
import { Configuration, FrontendApi, RegistrationFlow } from "@ory/client-fetch";
import { UserAuthCard } from "@ory/elements";

const kratos = new FrontendApi(
  new Configuration({ basePath: "/auth/.ory" })
);

export default function Registration() {
  const [flow, setFlow] = useState<RegistrationFlow | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const flowId = new URLSearchParams(window.location.search).get("flow");
    if (!flowId) {
      window.location.replace("/auth/.ory/self-service/registration/browser");
      return;
    }
    kratos.getRegistrationFlow({ id: flowId }).then(setFlow).catch(() => {
      setError("Registration session expired. Redirecting…");
      setTimeout(() => window.location.replace("/auth/.ory/self-service/registration/browser"), 1500);
    });
  }, []);

  if (error) return <p>{error}</p>;
  if (!flow) return null;

  return (
    <UserAuthCard
      flowType="registration"
      flow={flow}
      additionalProps={{
        loginURL: "/auth/login",
      }}
    />
  );
}
