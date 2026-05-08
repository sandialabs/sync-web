import { useEffect, useState } from "react";
import { Configuration, FrontendApi, SettingsFlow } from "@ory/client-fetch";
import { UserSettingsCard } from "@ory/elements";

const kratos = new FrontendApi(
  new Configuration({ basePath: "/auth/.ory" })
);

export default function Settings() {
  const [flow, setFlow] = useState<SettingsFlow | null>(null);
  const [error, setError] = useState<string | null>(null);

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

  if (error) return <p>{error}</p>;
  if (!flow) return null;

  // Username is immutable post-registration; hide it from the self-service settings UI.
  const filteredFlow = {
    ...flow,
    ui: {
      ...flow.ui,
      nodes: flow.ui.nodes.filter(
        (n) => !(n.group === "profile" && "name" in n.attributes && n.attributes.name === "traits.username")
      ),
    },
  };

  return <UserSettingsCard flow={filteredFlow} flowType="settings" />;
}
