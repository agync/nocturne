import { afterEach, describe, expect, it } from "vitest";
import { authHeaders, captureToken, currentToken } from "./httpAuth";

afterEach(() => {
  sessionStorage.clear();
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
