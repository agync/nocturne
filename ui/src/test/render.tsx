import { render } from "@testing-library/react";
import { App } from "../app/App";
import type { EngineClient } from "../engine/client";
import { createMockEngine } from "../engine/mock/engine";
import type { Platform, SystemTheme } from "../platform/types";

export interface FakePlatform extends Platform {
  hover(count: number | null): void;
  drop(paths: string[]): void;
  theme(theme: SystemTheme): void;
  handOff(question: string): void;
  /** What the quick-capture window last asked the main window to show. */
  opened(): string | null;
}

export function fakePlatform(initial: SystemTheme | null = null): FakePlatform {
  let onDrop: (paths: string[]) => void = () => undefined;
  let onHover: (count: number | null) => void = () => undefined;
  let onTheme: (theme: SystemTheme) => void = () => undefined;
  let onHandOff: (question: string) => void = () => undefined;
  let onOpen: (path: string) => void = () => undefined;
  let lastOpened: string | null = null;
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
    pickFolder: async () => null,
    openPath: async () => undefined,
    openOriginal: async () => undefined,
    openLogs: async () => undefined,
    originalSrc: (path) => `file:${path}`,
    closeQuickCapture: async () => undefined,
    handOffAsk: async () => undefined,
    onAskHandOff: (cb) => {
      onHandOff = cb;
      return () => undefined;
    },
    openInMain: async (path) => {
      lastOpened = path;
      onOpen(path);
    },
    onOpenInMain: (cb) => {
      onOpen = cb;
      return () => undefined;
    },
    opened: () => lastOpened,
    hover: (count) => {
      onHover(count);
    },
    drop: (paths) => {
      onDrop(paths);
    },
    theme: (theme) => {
      onTheme(theme);
    },
    handOff: (question) => onHandOff(question),
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
