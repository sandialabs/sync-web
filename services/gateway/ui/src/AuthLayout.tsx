import { ReactNode, useEffect, useMemo, useState } from "react";
import { ThemeProvider, defaultDarkTheme, defaultLightTheme } from "@ory/elements";
import "./AuthLayout.css";

const THEME_KEY = "sync-gateway-theme";

type ColorMode = "light" | "dark";

function getInitialTheme(): ColorMode {
  const stored = window.localStorage.getItem(THEME_KEY);
  if (stored === "light" || stored === "dark") return stored;
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

function getOryTheme(mode: ColorMode) {
  const base = mode === "dark" ? defaultDarkTheme : defaultLightTheme;

  return {
    ...base,
    fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif',
    accent: {
      ...base.accent,
      def: "#00add0",
      emphasis: "#0076a9",
      muted: mode === "dark" ? "#6fc9dc" : "#0076a9",
      subtle: mode === "dark" ? "#123943" : "#e6f8fb",
    },
    foreground: {
      ...base.foreground,
      def: mode === "dark" ? "#f0f3f8" : "#002b4c",
      muted: mode === "dark" ? "#a1abb8" : "#5d7084",
      subtle: mode === "dark" ? "#8f99a6" : "#7d8ea0",
    },
    background: {
      ...base.background,
      canvas: mode === "dark" ? "#181a1f" : "#ffffff",
      surface: mode === "dark" ? "#20242b" : "#ffffff",
      subtle: mode === "dark" ? "#252b34" : "#f8f8f8",
    },
    border: {
      ...base.border,
      def: mode === "dark" ? "#343b46" : "#d9e0e7",
    },
    text: {
      ...base.text,
      def: mode === "dark" ? "#f0f3f8" : "#002b4c",
    },
    input: {
      ...base.input,
      background: mode === "dark" ? "#181d24" : "#ffffff",
      disabled: mode === "dark" ? "#2b313a" : "#eef2f5",
      placeholder: mode === "dark" ? "#8f99a6" : "#7d8ea0",
      text: mode === "dark" ? "#f0f3f8" : "#002b4c",
    },
  };
}

interface AuthLayoutProps {
  active: "login" | "registration" | "recovery" | "settings";
  children: ReactNode;
  title: string;
}

export default function AuthLayout({ active, children, title }: AuthLayoutProps) {
  const [theme, setTheme] = useState<ColorMode>(() => getInitialTheme());
  const nextTheme = theme === "light" ? "dark" : "light";
  const oryTheme = useMemo(() => getOryTheme(theme), [theme]);

  useEffect(() => {
    window.localStorage.setItem(THEME_KEY, theme);
    document.documentElement.setAttribute("data-theme", theme);
  }, [theme]);

  return (
    <div className="auth-shell" data-theme={theme}>
      <header className="auth-toolbar">
        <div className="auth-toolbar-left">
          <img className="auth-toolbar-logo" src="/gateway-logo.png" alt="Synchronic Web" />
          <nav className="auth-toolbar-tabs" aria-label="Synchronic sections">
            <a className="auth-toolbar-pill" href="/gateway">Gateway Home</a>
            <a className="auth-toolbar-pill" href="/api/v1/docs">API Reference</a>
          </nav>
        </div>
        <div className="auth-toolbar-right">
          <button
            className="auth-theme-toggle"
            type="button"
            title={`Switch to ${nextTheme} mode`}
            aria-label={`Switch to ${nextTheme} mode`}
            onClick={() => setTheme(nextTheme)}
          >
            {theme === "light" ? "◐" : "◑"}
          </button>
        </div>
      </header>
      <ThemeProvider theme={theme} themeOverrides={oryTheme}>
        <main className="auth-main">
          <section className="auth-panel" aria-label={title}>
            <div className="auth-panel-header">
              <div className="auth-panel-kicker">
                {active === "settings" ? "Account" : "Authentication"}
              </div>
              <h1 className="auth-panel-title">{title}</h1>
            </div>
            <div className="auth-panel-body">{children}</div>
          </section>
        </main>
      </ThemeProvider>
    </div>
  );
}
