import { afterEach, describe, expect, it, vi } from "vitest";
import { authHeaders, captureToken, currentToken } from "./httpAuth";

afterEach(() => {
  sessionStorage.clear();
  localStorage.clear();
  vi.unstubAllEnvs();
  vi.resetModules();
  window.history.replaceState(null, "", "/");
});

describe("the preview token", () => {
  it("is taken from the fragment, kept for the tab and removed from the address", () => {
    window.history.replaceState(null, "", "/#token=abc123");
    expect(captureToken()).toBe("abc123");
    expect(window.location.hash).toBe("#/");
    expect(sessionStorage.getItem("nocturne.preview.token")).toBe("abc123");
    expect(currentToken()).toBe("abc123");
    expect(captureToken()).toBe("abc123");
  });

  it("is absent when no link carried it", () => {
    expect(captureToken()).toBeNull();
    expect(authHeaders(null)).toEqual({});
    expect(authHeaders("t")).toEqual({ authorization: "Bearer t" });
  });

  it("ignores a fragment that is only a route", () => {
    window.history.replaceState(null, "", "/#/today");
    expect(captureToken()).toBeNull();
    expect(window.location.hash).toBe("#/today");
  });
});

describe("the server token", () => {
  it("is kept in this browser, not only this tab, under its own name", async () => {
    vi.stubEnv("MODE", "server");
    vi.resetModules();
    const server = await import("./httpAuth");
    window.history.replaceState(null, "", "/#token=def456");
    expect(server.captureToken()).toBe("def456");
    expect(window.location.hash).toBe("#/");
    expect(localStorage.getItem("nocturne.server.token")).toBe("def456");
    expect(sessionStorage.getItem("nocturne.preview.token")).toBeNull();
    expect(server.currentToken()).toBe("def456");
  });
});
