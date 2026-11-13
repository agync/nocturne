import { Suspense, useEffect } from "react";
import { Router, useLocation } from "wouter";
import { useHashLocation } from "wouter/use-hash-location";
import { ToastProvider } from "../components/Toast/Toast";
import { t } from "../copy/t";
import type { EngineClient } from "../engine/client";
import { EngineProvider } from "../engine/context";
import { PlatformProvider } from "../platform/context";
import type { Platform } from "../platform/types";
import { findRoute } from "./routes";
import { Shell } from "./Shell";
import { useThemeSync } from "./theme";

interface AppProps {
  engine: EngineClient;
  platform: Platform;
}

const hrefs = (path: string) => `#${path}`;

function NotFound() {
  return (
    <section style={{ padding: "var(--s-6) var(--s-5)" }}>
      <h1 className="t-page-title">{t("screen.notFound.title")}</h1>
      <p className="t-ui-body" style={{ margin: "var(--s-3) 0", color: "var(--ink2)" }}>
        {t("screen.notFound.body")}
      </p>
      <a href="#/today">{t("screen.notFound.action")}</a>
    </section>
  );
}

function Routed() {
  const [location, navigate] = useLocation();
  useThemeSync();

  useEffect(() => {
    if (location === "/" || location === "") navigate("/today", { replace: true });
  }, [location, navigate]);

  const route = findRoute(location);
  if (location === "/" || location === "") return null;
  if (route && !route.shell) {
    const Bare = route.Component;
    return (
      <Suspense fallback={<p role="status">{t("screen.loading")}</p>}>
        <Bare />
      </Suspense>
    );
  }
  const Screen = route?.Component;
  return <Shell>{Screen ? <Screen /> : <NotFound />}</Shell>;
}

export function App({ engine, platform }: AppProps) {
  return (
    <EngineProvider engine={engine}>
      <PlatformProvider platform={platform}>
        <ToastProvider>
          <Router hook={useHashLocation} hrefs={hrefs}>
            <Routed />
          </Router>
        </ToastProvider>
      </PlatformProvider>
    </EngineProvider>
  );
}
