import AuthLayout from "../AuthLayout";

export default function Recovery() {
  return (
    <AuthLayout active="recovery" title="Password recovery">
      <p className="auth-error">
        Password recovery is handled by the system administrator for this deployment.
        Contact your system administrator to reset your password.
      </p>
    </AuthLayout>
  );
}
