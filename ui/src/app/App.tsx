import { Suspense, useEffect } from "react";
import type { BaseLocationHook } from "wouter";
import { Router, useLocation } from "wouter";
import { useHashLocation } from "wouter/use-hash-location";
import { ErrorBoundary } from "../components/ErrorBoundary/ErrorBoundary";
import { AppError } from "../components/ErrorBoundary/ScreenError";
import { ToastProvider } from "../components/Toast/Toast";
import { t } from "../copy/t";
import type { EngineClient } from "../engine/client";
import { EngineProvider, useEngine } from "../engine/context";
import { PlatformProvider } from "../platform/context";
import type { Platform } from "../platform/types";
import { NotFound } from "../screens/NotFound";
import { firstScreen } from "./firstScreen";
import { pathOf } from "./query";
import { findRoute } from "./routes";
import { Shell } from "./Shell";
import { useThemeSync } from "./theme";

interface AppProps {
  engine: EngineClient;
  platform: Platform;
}

const hrefs = (path: string) => `#${path}`;

/** The hash location without its `?query`, so routes match and `useQueryParam` reads the rest. */
const useHashPath: BaseLocationHook = (...args) => {
  const [location, navigate] = useHashLocation(...args);
  return [pathOf(location), navigate];
};

function Routed() {
  const [location, navigate] = useLocation();
  useThemeSync();

  const engine = useEngine();

  useEffect(() => {
    if (location !== "/" && location !== "") return;
    let live = true;
    engine
      .getSetup()
      .then(firstScreen, () => "/setup")
      .then((to) => {
        if (live) navigate(to, { replace: true });
      });
    return () => {
      live = false;
    };
  }, [engine, location, navigate]);

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
    <ErrorBoundary fallback={<AppError />}>
      <AppProviders engine={engine} platform={platform} />
    </ErrorBoundary>
  );
}

function AppProviders({ engine, platform }: AppProps) {
  return (
    <EngineProvider engine={engine}>
      <PlatformProvider platform={platform}>
        <ToastProvider>
          <Router hook={useHashPath} hrefs={hrefs}>
            <Routed />
          </Router>
        </ToastProvider>
      </PlatformProvider>
    </EngineProvider>
  );
}
