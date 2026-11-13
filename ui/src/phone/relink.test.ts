// @vitest-environment node
import { describe, expect, it } from "vitest";
import { createPhoneEngine } from "./client";
import { offerFromFragment, Pairing } from "./pairing";
import { addressFromRelinkText, addressFromWords, relink } from "./relink";
import { FakeComputer } from "./testing/fakeComputer";
import { memoryKeyStore } from "./testing/memoryStore";

async function pairedTo(host: string) {
  const computer = await FakeComputer.create({ host });
  const offer = await computer.offer();
  computer.autoDecide = "allow";
  const store = memoryKeyStore();
  const result = await (
    await Pairing.fromLink(offerFromFragment(new URL(offer.link).hash) ?? fail(), {
      env: computer.env,
      host,
      origin: `https://${host}`,
      store,
    })
  ).connect("Pixel", () => undefined);
  if (result.kind !== "allowed") throw new Error(result.kind);
  return { computer, store };
}

function fail(): never {
  throw new Error("no offer");
}

describe("re-link", () => {
  it("relink.scan_updates_address_without_approval: the new address is kept once Hello opens", async () => {
    const { computer, store } = await pairedTo("old-old-old-old.trycloudflare.com");
    const next = "new-new-new-new.trycloudflare.com";
    const before = computer.devices.size;
    const result = await relink(store, next, computer.env);
    expect(result).toEqual({ ok: true, address: next });
    expect(store.current()?.address).toBe(next);
    expect(store.current()?.homeOrigin).toBe("https://old-old-old-old.trycloudflare.com");
    expect(computer.devices.size).toBe(before);
    expect(computer.offers.size).toBe(1);
  });

  it("an address that does not answer is not kept", async () => {
    const { computer, store } = await pairedTo("old-old-old-old.trycloudflare.com");
    computer.networkDown = true;
    const result = await relink(store, "new-new-new-new.trycloudflare.com", computer.env, {
      handshakeMs: 200,
    });
    expect(result.ok).toBe(false);
    expect(store.current()?.address).toBe("old-old-old-old.trycloudflare.com");
  });

  it("an engine pointed at the stored address follows a re-link", async () => {
    const { computer, store } = await pairedTo("old-old-old-old.trycloudflare.com");
    await relink(store, "new-new-new-new.trycloudflare.com", computer.env);
    const urls: string[] = [];
    const engine = createPhoneEngine(store, () => store.current()?.address ?? "", {
      env: {
        ...computer.env,
        WebSocket: undefined,
        fetch: (input, init) => {
          urls.push(String(input));
          return computer.env.fetch(input, init);
        },
      },
    });
    await engine.connect();
    expect(urls.every((u) => new URL(u).host === "new-new-new-new.trycloudflare.com")).toBe(true);
    engine.close();
  });

  it("relink.typed_words_build_the_address", () => {
    expect(addressFromWords(["River", " copper", "mean ", "VAULT"])).toBe(
      "river-copper-mean-vault.trycloudflare.com",
    );
    expect(addressFromWords(["river", "copper", "mean"])).toBeNull();
    expect(addressFromWords(["river", "copper", "mean", "va/ult"])).toBeNull();
    expect(addressFromWords(["river", "copper", "mean", ""])).toBeNull();
  });

  it("reads a code only when it is a re-link for its own address", () => {
    const host = "river-copper-mean-vault.trycloudflare.com";
    expect(addressFromRelinkText(`https://${host}/r#${host}`)).toBe(host);
    expect(addressFromRelinkText(`https://${host}/r#other-host.trycloudflare.com`)).toBeNull();
    expect(addressFromRelinkText(`http://${host}/r#${host}`)).toBeNull();
    expect(addressFromRelinkText(`https://${host}/p#v1.aaa.bbb`)).toBeNull();
    expect(addressFromRelinkText("not a link")).toBeNull();
  });

  it("a re-link code cannot start a pairing", () => {
    const host = "river-copper-mean-vault.trycloudflare.com";
    expect(offerFromFragment(new URL(`https://${host}/r#${host}`).hash)).toBeNull();
  });
});
