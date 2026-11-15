import { renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { applyTheme } from "../../app/theme";
import { useNight } from "./useNight";

afterEach(() => applyTheme(null));

describe("useNight", () => {
  it("gives back the theme that arrived while setup was open", async () => {
    const root = document.documentElement;
    applyTheme(null);
    const night = renderHook(() => useNight(true));
    expect(root.getAttribute("data-theme")).toBe("dark");
    // The system theme is read after setup opened, as it can be at launch.
    applyTheme("light");
    await waitFor(() => expect(root.getAttribute("data-theme")).toBe("dark"));
    night.unmount();
    expect(root.getAttribute("data-theme")).toBe("light");
  });
});
