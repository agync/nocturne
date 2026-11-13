import { describe, expect, it } from "vitest";
import type { CheckOutcome, OnlineForm } from "../bindings";
import { OnlineSim } from "./online";

const form = (patch: Partial<OnlineForm> = {}): OnlineForm => ({
  baseUrl: "https://api.example.com/v1",
  key: "sk-example-not-a-real-key",
  model: "llama-3.3-70b-instruct",
  ...patch,
});

const settings = {
  phones: true,
  sendSensitive: false,
  limit: { kind: "questions" as const, questions: 200, usd: null },
};

const sim = (scenario = "off") => new OnlineSim({ scenario, checkMs: 0 });

describe("mock.online", () => {
  it("setup_path: off, checked, turned on, turned off, forgotten", async () => {
    const online = sim();
    expect(online.view().state).toEqual({ type: "off" });
    expect(online.view().setUp).toBe(false);

    await expect(online.turnOn(settings)).rejects.toThrow();

    expect(await online.check(form())).toEqual({
      type: "works",
      host: "api.example.com",
      model: "llama-3.3-70b-instruct",
    });
    const on = await online.turnOn(settings);
    expect(on.state).toEqual({ type: "on" });
    expect(on.setUp).toBe(true);
    expect(on.host).toBe("api.example.com");
    expect(on.keyStorage).toBe("keychain");
    expect(on.onSince).not.toBeNull();

    const off = await online.turnOff(false);
    expect(off.state).toEqual({ type: "off" });
    expect(off.setUp).toBe(true);
    expect(off.lastOnAt).not.toBeNull();

    const back = await online.turnOn(settings);
    expect(back.state).toEqual({ type: "on" });

    const forgotten = await online.turnOff(true);
    expect(forgotten.setUp).toBe(false);
    expect(forgotten.host).toBeNull();
    expect(forgotten.keyStorage).toBeNull();
  });

  it("check_outcomes: each way a check ends", async () => {
    const online = sim();
    const outcome = async (patch: Partial<OnlineForm>): Promise<CheckOutcome["type"]> =>
      (await online.check(form(patch))).type;
    expect(await outcome({})).toBe("works");
    expect(await outcome({ baseUrl: "http://api.example.com/v1" })).toBe("notHttps");
    expect(await outcome({ baseUrl: "https://localhost/v1" })).toBe("addressNotAllowed");
    expect(await outcome({ baseUrl: "https://offline.example.com" })).toBe("noInternet");
    expect(await outcome({ baseUrl: "https://unreachable.example.com" })).toBe("unreachable");
    expect(await outcome({ baseUrl: "not an address" })).toBe("unreachable");
    expect(await outcome({ key: "sk-refused" })).toBe("keyRefused");
    expect(await outcome({ key: "sk-nocredit" })).toBe("noCredit");
    expect(await outcome({ key: "sk-busy" })).toBe("busy");
    expect(await outcome({ key: "sk-broken" })).toBe("serverError");
    expect(await outcome({ key: "sk-empty" })).toBe("emptyReply");
    expect(await outcome({ model: "no-such-missing-name" })).toBe("modelMissing");
  });

  it("check_outcomes: a failed check leaves nothing to turn on, and the key is never kept", async () => {
    const online = sim();
    await online.check(form({ key: "sk-refused" }));
    await expect(online.turnOn(settings)).rejects.toThrow();
    expect(JSON.stringify(online.view())).not.toContain("sk-");
    expect(JSON.stringify(online.checks)).not.toContain("sk-");
  });

  it("check_outcomes: a check can be cancelled and setup abandoned", async () => {
    const online = new OnlineSim({ checkMs: 60_000 });
    const pending = online.check(form());
    await online.cancelCheck();
    expect(await pending).toEqual({ type: "cancelled" });
    expect(online.view().state).toEqual({ type: "off" });
    await online.abandonSetup();
    expect(online.abandoned).toBe(1);
  });

  it("states: paused, refused, no credit, problem and cost", () => {
    expect(sim("limit").view().state).toEqual({
      type: "paused",
      pause: { type: "limit", until: expect.any(String) },
    });
    expect(sim("refused").view().state).toEqual({ type: "paused", pause: { type: "keyRefused" } });
    expect(sim("noCredit").view().state).toEqual({ type: "paused", pause: { type: "noCredit" } });
    expect(sim("problem").view().state.type).toBe("problem");
    expect(sim("on").view().costReported).toBe(false);
    expect(sim("cost").view().costReported).toBe(true);
    expect(sim("cost").view().spentThisMonthUsd).toBe(0.42);
  });

  it("states: raising the limit lifts a pause, and a lower one pauses", async () => {
    const online = sim("limit");
    const lifted = await online.update({
      phones: null,
      sendSensitive: null,
      limit: { kind: "questions", questions: 500, usd: null },
    });
    expect(lifted.state).toEqual({ type: "on" });
    const paused = await online.update({
      phones: null,
      sendSensitive: null,
      limit: { kind: "questions", questions: 100, usd: null },
    });
    expect(paused.state.type).toBe("paused");
  });
});

describe("mock.online.events", () => {
  it("emits online-changed when the state changes", async () => {
    const { createMockEngine } = await import("./engine");
    const engine = createMockEngine();
    const seen: string[] = [];
    engine.on("online-changed", (view) => seen.push(view.state.type));
    engine.online.setCheckMs(0);
    await engine.checkOnline(form());
    await engine.turnOnOnline(settings);
    await engine.turnOffOnline(false);
    expect(seen).toEqual(["checking", "off", "on", "off"]);
  });

  it("record: keeps the real question and sentences, and says what an empty or unsure send was", async () => {
    const online = sim("on");
    const a = online.record("When is the dentist?", "done", ["The dentist is on Friday."]);
    const b = online.record("Anything on Sunday?", "notSure", ["Maybe church."]);
    const c = online.record("Private thing?", "done", []);
    const [sa, sb, sc] = await Promise.all(
      [a, b, c].map(async (id) => (await online.sends(10, id + 1)).find((x) => x.id === id)),
    );
    expect(sa?.question).toBe("When is the dentist?");
    expect(sa?.lines).toContainEqual({
      type: "memory",
      n: 1,
      text: "The dentist is on Friday.",
      state: "same",
    });
    expect(sa?.lines).toContainEqual({ type: "text", text: "Question: When is the dentist?" });
    expect(sa?.result).toEqual({ type: "answered", receipts: 1 });
    expect(sb?.result).toEqual({ type: "notSure" });
    expect(sc?.result).toEqual({ type: "notSent", reason: "keptHere" });
    expect(sc?.lines).toEqual([]);
    expect(sc?.bytesUp).toBe(0);
  });
});
