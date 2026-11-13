import { render } from "@testing-library/react";
import { App } from "../app/App";
import type { EngineClient } from "../engine/client";
import { createMockEngine } from "../engine/mock/engine";
import type { Platform, SystemTheme } from "../platform/types";

export interface FakePlatform extends Platform {
  hover(count: number | null): void;
  drop(paths: string[]): void;
  theme(theme: SystemTheme): void;
}

export function fakePlatform(initial: SystemTheme | null = null): FakePlatform {
  let onDrop: (paths: string[]) => void = () => undefined;
  let onHover: (count: number | null) => void = () => undefined;
  let onTheme: (theme: SystemTheme) => void = () => undefined;
  return {
    onFileDrop(drop, hover) {
      onDrop = drop;
      onHover = hover;
      return () => undefined;
    },
    onThemeChanged(cb) {
      onTheme = cb;
      return () => undefined;
    },
    currentTheme: async () => initial,
    pickFiles: async () => [],
    hover: (count) => {
      onHover(count);
    },
    drop: (paths) => {
      onDrop(paths);
    },
    theme: (theme) => {
      onTheme(theme);
    },
  };
}

export function setViewport(width: number) {
  Object.defineProperty(window, "innerWidth", { configurable: true, value: width });
  window.dispatchEvent(new Event("resize"));
}

export function renderApp(
  options: { hash?: string; engine?: EngineClient; platform?: Platform } = {},
) {
  window.location.hash = options.hash ?? "#/today";
  const engine = options.engine ?? createMockEngine();
  const platform = options.platform ?? fakePlatform();
  return { engine, platform, ...render(<App engine={engine} platform={platform} />) };
}
