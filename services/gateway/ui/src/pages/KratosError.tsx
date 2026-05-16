import { useEffect, useState } from "react";
import { Configuration, FlowError, FrontendApi } from "@ory/client-fetch";
import AuthLayout from "../AuthLayout";

const kratos = new FrontendApi(
  new Configuration({ basePath: "/auth/.ory" })
);

export default function KratosError() {
  const [flowError, setFlowError] = useState<FlowError | null>(null);
  const [notFound, setNotFound] = useState(false);

  useEffect(() => {
    const id = new URLSearchParams(window.location.search).get("id");
    if (!id) {
      setNotFound(true);
      return;
    }
    kratos.getFlowError({ id }).then(setFlowError).catch(() => setNotFound(true));
  }, []);

  if (notFound) {
    return (
      <AuthLayout active="login" title="Error">
        <p className="auth-error">An unknown error occurred.</p>
        <p><a href="/auth/login">Back to login</a></p>
      </AuthLayout>
    );
  }
  if (!flowError) return null;

  const message =
    typeof flowError.error === "object" && flowError.error !== null
      ? (flowError.error as { reason?: string; message?: string }).reason ??
        (flowError.error as { reason?: string; message?: string }).message ??
        JSON.stringify(flowError.error)
      : String(flowError.error ?? "An unknown error occurred.");

  return (
    <AuthLayout active="login" title="Error">
      <p className="auth-error">{message}</p>
      <p><a href="/auth/login">Back to login</a></p>
    </AuthLayout>
  );
}
