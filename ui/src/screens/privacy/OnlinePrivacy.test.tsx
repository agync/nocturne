import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { LedgerEntry, PrivacyView } from "../../engine/bindings";
import { createMockEngine } from "../../engine/mock/engine";
import type { OnlineScenario } from "../../engine/mock/online";
import { renderApp, setViewport } from "../../test/render";

beforeEach(() => setViewport(1280));
afterEach(() => {
  window.location.hash = "";
  vi.restoreAllMocks();
});

const ledger = (
  kind: LedgerEntry["kind"],
  label: string,
  bytes: number,
  at: string,
): LedgerEntry => ({
  kind,
  label,
  bytes,
  at,
});

const ON: Partial<PrivacyView> = {
  onlineOn: true,
  onlineHost: "api.example.com",
  onlineLastOnAt: "2026-11-13T09:14:00Z",
  sentOnlineBytes: 46_000,
  onlineQuestions: 38,
  ledger: [
    ledger("onlineConnected", "api.example.com", 0, "2026-11-02T10:00:00Z"),
    ledger("onlineOnOff", "on", 0, "2026-11-02T10:00:00Z"),
    ledger("modelDownload", "Qwen3.5-9B-GGUF", 5_900_000_000, "2026-11-01T10:00:00Z"),
  ],
};

const OFF_AFTER_ON: Partial<PrivacyView> = {
  ...ON,
  onlineOn: false,
  onlineLastOnAt: "2026-11-12T20:00:00Z",
  ledger: [ledger("onlineOnOff", "off", 0, "2026-11-12T20:00:00Z"), ...(ON.ledger ?? [])],
};

async function open(patch: Partial<PrivacyView> | null, scenario: OnlineScenario = "off") {
  const engine = createMockEngine({ sentMeasured: false });
  engine.online.setScenario(scenario);
  if (patch) {
    const real = engine.getPrivacy.bind(engine);
    vi.spyOn(engine, "getPrivacy").mockImplementation(async () => ({
      ...(await real()),
      ...patch,
    }));
  }
  renderApp({ hash: "#/privacy", engine });
  await screen.findByRole("heading", { level: 1 });
  return engine;
}

describe("privacy with online answers", () => {
  it("never_set_up: says nothing about online answers", async () => {
    await open(null);
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(
      "Only downloads you agreed to.",
    );
    expect(screen.queryByText("Sent for online answers")).toBeNull();
    expect(screen.queryByRole("region", { name: "Online answers" })).toBeNull();
    expect(screen.queryByText(/Online answers, if you turn them on/)).toBeNull();
  });

  it("privacy.online_on_names_host: names the provider and what it can and cannot see", async () => {
    await open(ON);
    expect(await screen.findByRole("heading", { level: 1 })).toHaveTextContent(
      "Only downloads you agreed to, and online answers.",
    );
    const section = await screen.findByRole("region", { name: "Online answers" });
    expect(section).toHaveTextContent(
      "Online answers are on. Your questions, and the few things that answer them, go from Nocturne’s engine on this computer to your provider (api.example.com). Nothing else is sent.",
    );
    for (const line of [
      "Your question",
      "The lines sent with it",
      "When you ask",
      "This computer’s internet address",
      "Your photos, recordings and documents",
      "Everything else you kept",
    ]) {
      expect(within(section).getByText(line)).toBeInTheDocument();
    }
    expect(section).toHaveTextContent("Nocturne can’t check what your provider keeps.");
    expect(within(section).getByRole("link", { name: "See what was sent" })).toHaveAttribute(
      "href",
      "#/settings/online/sent",
    );
    expect(within(section).getByRole("link", { name: "Turn off" })).toHaveAttribute(
      "href",
      "#/settings/online",
    );
    expect(screen.getByText("46 kB in 38 questions")).toBeInTheDocument();
    expect(
      screen.getByText(
        /send your question and the few things needed to answer it to your provider/,
      ),
    ).toBeInTheDocument();
  });

  it("privacy.off_after_on: goes back to the plain headline and says when it was last on", async () => {
    await open(OFF_AFTER_ON);
    expect(await screen.findByRole("heading", { level: 1 })).toHaveTextContent(
      "Only downloads you agreed to.",
    );
    const section = await screen.findByRole("region", { name: "Online answers" });
    expect(section).toHaveTextContent(
      "Online answers are off. Questions are answered on this computer.",
    );
    expect(within(section).queryByRole("link", { name: "Turn off" })).toBeNull();
    expect(within(section).getByRole("link", { name: "See what was sent" })).toBeInTheDocument();
    expect(screen.getByText("46 kB, last on 12 Nov")).toBeInTheDocument();
  });

  it("privacy.ledger_online_and_downloads: lists turning on and off, connecting and each download", async () => {
    await open(OFF_AFTER_ON);
    const list = await screen.findByRole("region", { name: "Downloaded, with your say-so" });
    expect(within(list).getByText("Online answers off")).toBeInTheDocument();
    expect(within(list).getByText("Online answers on")).toBeInTheDocument();
    expect(within(list).getByText("Connected to api.example.com")).toBeInTheDocument();
    expect(within(list).getByText("Downloaded for a job: Qwen3.5-9B-GGUF")).toBeInTheDocument();
    expect(within(list).getByText("5.9 GB · 1 Nov")).toBeInTheDocument();
    expect(within(list).getAllByText("2 Nov")).toHaveLength(2);
  });

  it("says the export holds the list of what was sent and never the key", async () => {
    await open(ON);
    expect(
      await screen.findByText(/the list of what was sent online\. Never the key\./),
    ).toBeInTheDocument();
  });

  it("with Away from home on too, the headline names both", async () => {
    await open({ ...ON, remoteOn: true, remoteConnection: "simple" });
    expect(await screen.findByRole("heading", { level: 1 })).toHaveTextContent(
      "Only downloads you agreed to, Away from home, and online answers.",
    );
  });
});

describe("delete everything", () => {
  const openDialog = async () => {
    await userEvent.click(await screen.findByRole("button", { name: "Delete everything…" }));
    return screen.findByRole("dialog");
  };

  it("delete.mentions_key_and_log_only_when_set_up: names the key and the log once online answers was set up", async () => {
    await open(ON, "on");
    const dialog = await openDialog();
    expect(dialog).toHaveTextContent(
      "This also forgets the key for your provider (api.example.com) on this computer, and the list of what was sent online.",
    );
  });

  it("leaves the key out when none is stored, even after a set-up", async () => {
    await open(ON);
    const dialog = await openDialog();
    expect(dialog).not.toHaveTextContent(/This also forgets the key/);
  });

  it("does not mention a key or a log when online answers was never set up", async () => {
    await open(null);
    const dialog = await openDialog();
    expect(dialog).not.toHaveTextContent(/key/);
    expect(dialog).not.toHaveTextContent(/sent online/);
    await waitFor(() =>
      expect(within(dialog).getByRole("button", { name: "Keep everything" })).toBeVisible(),
    );
  });
});
