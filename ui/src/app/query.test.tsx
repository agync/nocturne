import { act, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { createMockEngine } from "../engine/mock/engine";
import { fakePlatform, renderApp, setViewport } from "../test/render";
import { goTo, hashHref, hashParams, pathOf, useQueryParam } from "./query";

afterEach(() => {
  window.location.hash = "";
});

function Param({ name }: { name: string }) {
  const value = useQueryParam(name);
  return <p>{`${name}=${value ?? "none"}`}</p>;
}

describe("hash query", () => {
  it("splits a location into its path and parameters", () => {
    expect(pathOf("/memories?q=bakery")).toBe("/memories");
    expect(pathOf("/memories")).toBe("/memories");
    expect(hashParams("#/memories?q=bakery&show=all").get("show")).toBe("all");
    expect(hashParams("#/memories").get("q")).toBeNull();
  });

  it("builds links and leaves empty values out", () => {
    expect(hashHref("/memories", { q: "bake ry", show: null, x: "" })).toBe("#/memories?q=bake+ry");
    expect(hashHref("/today")).toBe("#/today");
  });

  it("reads a parameter and follows the hash as it changes", async () => {
    window.location.hash = "#/memories?q=bakery";
    render(<Param name="q" />);
    expect(screen.getByText("q=bakery")).toBeInTheDocument();
    act(() => {
      goTo("/memories", { q: "dentist" });
      window.dispatchEvent(new HashChangeEvent("hashchange"));
    });
    await waitFor(() => expect(screen.getByText("q=dentist")).toBeInTheDocument());
    act(() => {
      goTo("/memories");
      window.dispatchEvent(new HashChangeEvent("hashchange"));
    });
    await waitFor(() => expect(screen.getByText("q=none")).toBeInTheDocument());
  });
});

describe("a screen opened with a query", () => {
  it("still renders the screen and marks the rail entry as current", async () => {
    setViewport(1280);
    renderApp({
      hash: "#/memories?q=bakery",
      engine: createMockEngine(),
      platform: fakePlatform(),
    });
    expect(
      await screen.findByRole("heading", { level: 1, name: "What Nocturne remembers" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /^What Nocturne remembers/ })).toHaveAttribute(
      "aria-current",
      "page",
    );
  });
});
