import { renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, it } from "vitest";
import { EngineProvider } from "../engine/context";
import { createMockEngine } from "../engine/mock/engine";
import { PlatformProvider } from "../platform/context";
import type { Platform, SystemTheme } from "../platform/types";
import { applyTheme, resolveTheme, useThemeSync } from "./theme";

function fakePlatform(initial: SystemTheme | null) {
  let listener: ((t: SystemTheme) => void) | null = null;
  const platform: Platform = {
    onFileDrop: () => () => undefined,
    onThemeChanged: (cb) => {
      listener = cb;
      return () => {
        listener = null;
      };
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
    openInMain: async () => undefined,
    onOpenInMain: () => () => undefined,
    onAskHandOff: () => () => undefined,
  };
  return { platform, change: (t: SystemTheme) => listener?.(t) };
}

afterEach(() => document.documentElement.removeAttribute("data-theme"));

describe("resolveTheme", () => {
  it("lets the setting win, then the platform, then leaves it to the system", () => {
    expect(resolveTheme("dark", "light")).toBe("dark");
    expect(resolveTheme("light", "dark")).toBe("light");
    expect(resolveTheme("system", "dark")).toBe("dark");
    expect(resolveTheme("system", null)).toBeNull();
  });

  it("sets and removes the attribute", () => {
    applyTheme("dark");
    expect(document.documentElement).toHaveAttribute("data-theme", "dark");
    applyTheme(null);
    expect(document.documentElement).not.toHaveAttribute("data-theme");
  });
});

describe("useThemeSync", () => {
  const wrap =
    (engine: ReturnType<typeof createMockEngine>, platform: Platform) =>
    ({ children }: { children: ReactNode }) => (
      <EngineProvider engine={engine}>
        <PlatformProvider platform={platform}>{children}</PlatformProvider>
      </EngineProvider>
    );

  it("follows the platform theme while the setting is system, and its changes", async () => {
    const { platform, change } = fakePlatform("dark");
    renderHook(() => useThemeSync(), { wrapper: wrap(createMockEngine(), platform) });
    await waitFor(() => expect(document.documentElement).toHaveAttribute("data-theme", "dark"));
    change("light");
    await waitFor(() => expect(document.documentElement).toHaveAttribute("data-theme", "light"));
  });

  it("forces the theme from settings over the platform", async () => {
    const engine = createMockEngine();
    await engine.updateSettings({
      pageTime: null,
      delivery: null,
      autoPrint: null,
      voice: null,
      voiceSpeed: null,
      shortcut: null,
      inboxFolder: null,
      startAtLogin: null,
      lettingGo: null,
      theme: "light",
      language: null,
      checkUpdates: null,
      reminders: null,
      lock: null,
      agents: null,
    });
    const { platform } = fakePlatform("dark");
    renderHook(() => useThemeSync(), { wrapper: wrap(engine, platform) });
    await waitFor(() => expect(document.documentElement).toHaveAttribute("data-theme", "light"));
  });
});
