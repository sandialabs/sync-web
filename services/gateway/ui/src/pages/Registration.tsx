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

  return (
    <UserAuthCard
      flowType="registration"
      flow={flow}
      additionalProps={{
        loginURL,
      }}
    />
  );
}
