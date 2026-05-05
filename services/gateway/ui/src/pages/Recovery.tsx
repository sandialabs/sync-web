import { useEffect, useState } from "react";
import { Configuration, FrontendApi, RecoveryFlow } from "@ory/client-fetch";
import { UserAuthCard } from "@ory/elements";

const kratos = new FrontendApi(
  new Configuration({ basePath: "/auth/.ory" })
);

export default function Recovery() {
  const [flow, setFlow] = useState<RecoveryFlow | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const flowId = new URLSearchParams(window.location.search).get("flow");
    if (!flowId) {
      window.location.replace("/auth/.ory/self-service/recovery/browser");
      return;
    }
    kratos.getRecoveryFlow({ id: flowId }).then(setFlow).catch(() => {
      setError("Recovery session expired. Redirecting…");
      setTimeout(() => window.location.replace("/auth/.ory/self-service/recovery/browser"), 1500);
    });
  }, []);

  if (error) return <p>{error}</p>;
  if (!flow) return null;

  return (
    <UserAuthCard
      flowType="recovery"
      flow={flow}
      additionalProps={{
        loginURL: "/auth/login",
      }}
    />
  );
}
