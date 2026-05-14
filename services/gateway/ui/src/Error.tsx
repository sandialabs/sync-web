import AuthLayout from "./AuthLayout";

export default function ErrorPage() {
  return (
    <AuthLayout active="login" title="Page not found">
      <div className="auth-error">
        <p>Page not found.</p>
        <p>
          <a href="/auth/login">Back to login</a>
        </p>
      </div>
    </AuthLayout>
  );
}
