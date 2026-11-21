import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { EngineError } from "../../engine/client";
import { NOW, openPhones, phonesEngine } from "./testing";

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(new Date(NOW));
});
afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  window.location.hash = "";
});

const open = async (engine = phonesEngine()) => {
  await openPhones("/settings/phones", "From your phones", engine);
  await userEvent.click(
    screen.getByRole("button", { name: "Delete what came in from Sam’s phone…" }),
  );
  return within(await screen.findByRole("dialog"));
};

describe("delete what came in", () => {
  it("lists the calendar and starts on Keep them", async () => {
    const dialog = await open();
    expect(
      await dialog.findByText(
        "41 appointments from Sam’s phone’s calendar, and their earlier versions",
      ),
    ).toBeInTheDocument();
    expect(dialog.getByText("What Nocturne learned from them")).toBeInTheDocument();
    expect(
      dialog.getByText(
        "2 things Nocturne learned also come from things you kept. They stay; only the phone’s receipt goes.",
      ),
    ).toBeInTheDocument();
    expect(
      dialog.getByText("6 lines on your past pages cite them and will say (forgotten)."),
    ).toBeInTheDocument();
    expect(dialog.getByText(/Your phone’s calendar isn’t touched\./)).toBeInTheDocument();
    expect(
      dialog.getByText(/Anything you exported or printed is out of reach/),
    ).toBeInTheDocument();
    expect(
      dialog.getByRole("checkbox", { name: "Also stop taking the calendar from Sam’s phone" }),
    ).toBeChecked();
    expect(
      dialog.getByText(
        "If you leave this off, the next two weeks come back the next time Sam’s phone sends.",
      ),
    ).toBeInTheDocument();
    expect(dialog.getByRole("button", { name: "Keep them" })).toHaveFocus();
    expect(dialog.getByRole("button", { name: "Delete" })).toHaveAttribute(
      "data-variant",
      "forget",
    );
  });

  it("deletes with the preview's token and stops the calendar by default", async () => {
    const engine = phonesEngine();
    const apply = vi.spyOn(engine, "deletePhoneData");
    const preview = vi.spyOn(engine, "previewPhoneDelete");
    const dialog = await open(engine);
    await dialog.findByText(/41 appointments/);
    expect(preview).toHaveBeenCalledWith({
      deviceId: "pixel",
      source: "calendar",
      kinds: null,
      before: null,
    });
    await userEvent.click(dialog.getByRole("button", { name: "Delete" }));
    await waitFor(() =>
      expect(apply).toHaveBeenCalledWith(expect.stringMatching(/^phones-delete-/), true),
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(
      await screen.findByRole("button", { name: "Take the calendar from Sam’s phone again" }),
    ).toBeInTheDocument();
  });

  it("keeps the calendar taking when the tick is off, and changes nothing on Keep them", async () => {
    const engine = phonesEngine();
    const apply = vi.spyOn(engine, "deletePhoneData");
    const dialog = await open(engine);
    await dialog.findByText(/41 appointments/);
    await userEvent.click(
      dialog.getByRole("checkbox", { name: "Also stop taking the calendar from Sam’s phone" }),
    );
    await userEvent.click(dialog.getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(apply).toHaveBeenCalledWith(expect.any(String), false));
  });

  it("Keep them closes without deleting", async () => {
    const engine = phonesEngine();
    const apply = vi.spyOn(engine, "deletePhoneData");
    const dialog = await open(engine);
    await userEvent.click(dialog.getByRole("button", { name: "Keep them" }));
    expect(apply).not.toHaveBeenCalled();
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("previews again when the token has expired", async () => {
    const engine = phonesEngine();
    const preview = vi.spyOn(engine, "previewPhoneDelete");
    vi.spyOn(engine, "deletePhoneData").mockRejectedValueOnce(
      new EngineError("tokenExpired", "preview again"),
    );
    const dialog = await open(engine);
    await dialog.findByText(/41 appointments/);
    await userEvent.click(dialog.getByRole("button", { name: "Delete" }));
    expect(
      await dialog.findByText(
        "That list was out of date, so Nocturne refreshed it. Check it, then delete.",
      ),
    ).toBeInTheDocument();
    expect(preview).toHaveBeenCalledTimes(2);
    await userEvent.click(await dialog.findByRole("button", { name: "Delete" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("asks which source when the phone brings both, and words Health by its kinds", async () => {
    const engine = phonesEngine();
    const dialog = await open(engine);
    await dialog.findByText(/41 appointments/);
    await userEvent.click(dialog.getByRole("radio", { name: "Health" }));
    expect(
      await screen.findByRole("dialog", { name: "Delete the health readings from Sam’s phone?" }),
    ).toBeInTheDocument();
    expect(
      await dialog.findByText(/readings of sleep, heart rate.* from Sam’s phone$/),
    ).toBeInTheDocument();
    expect(
      dialog.getByText(
        /Health on your phone isn’t touched\. Readings you delete here aren’t sent again\./,
      ),
    ).toBeInTheDocument();
    expect(dialog.queryByRole("checkbox")).toBeNull();
  });

  it("keeps the Health list and token when the calendar answer arrives late", async () => {
    const engine = phonesEngine();
    const apply = vi.spyOn(engine, "deletePhoneData");
    const real = engine.previewPhoneDelete.bind(engine);
    const tokens: Record<string, string> = {};
    let release: () => void = () => undefined;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    vi.spyOn(engine, "previewPhoneDelete").mockImplementation(async (request) => {
      const answer = await real(request);
      tokens[request.source] = answer.token;
      if (request.source === "calendar") await gate;
      return answer;
    });
    const dialog = await open(engine);
    await userEvent.click(dialog.getByRole("radio", { name: "Health" }));
    await dialog.findByText(/readings of sleep, heart rate/);
    release();
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(dialog.getByText(/readings of sleep, heart rate/)).toBeInTheDocument();
    expect(dialog.queryByText(/appointments from/)).toBeNull();
    await userEvent.click(dialog.getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(apply).toHaveBeenCalledTimes(1));
    expect(tokens.health).toBeDefined();
    expect(apply).toHaveBeenCalledWith(tokens.health, false);
  });

  it("holds Delete while the list for the chosen source is still coming", async () => {
    const engine = phonesEngine();
    const real = engine.previewPhoneDelete.bind(engine);
    let release: () => void = () => undefined;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    vi.spyOn(engine, "previewPhoneDelete").mockImplementation(async (request) => {
      if (request.source === "health") await gate;
      return real(request);
    });
    const dialog = await open(engine);
    await dialog.findByText(/41 appointments/);
    await userEvent.click(dialog.getByRole("radio", { name: "Health" }));
    expect(dialog.getByRole("button", { name: "Delete" })).toBeDisabled();
    release();
    await dialog.findByText(/readings of sleep/);
    expect(dialog.getByRole("button", { name: "Delete" })).toBeEnabled();
  });
});
