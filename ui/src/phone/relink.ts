// Re-linking a paired phone to its computer's new address. The code carries only the address:
// `https://<address>/r#<address>`. The phone opens a session there with the keys it already has
// and keeps the address only after the computer's Hello opens; nothing is approved.

import { type Env, OpenError, type OpenFailure, Session } from "./channel";
import type { KeyStore } from "./engine";
import * as P from "./protocol";

/** Quick tunnel addresses are four words under this name. */
export const TUNNEL_SUFFIX = ".trycloudflare.com";

const HOST_RE = /^(?=.{4,253}$)[a-z0-9]([a-z0-9-]*[a-z0-9])?(\.[a-z0-9]([a-z0-9-]*[a-z0-9])?)+$/;

/** The address in a re-link code, or null when the text is not one. */
export function addressFromRelinkText(text: string): string | null {
  let url: URL;
  try {
    url = new URL(text.trim());
  } catch {
    return null;
  }
  if (url.protocol !== "https:" || url.pathname !== P.PATH_RELINK || url.search !== "") return null;
  const address = decodeURIComponent(url.hash.replace(/^#/, "")).toLowerCase();
  if (!HOST_RE.test(address) || address !== url.host.toLowerCase()) return null;
  return address;
}

/** Four typed words, in any case, make the address; fewer or more, or odd characters, make none. */
export function addressFromWords(words: string[]): string | null {
  const cleaned = words.map((w) => w.trim().toLowerCase());
  if (cleaned.length !== 4 || cleaned.some((w) => !/^[a-z0-9]{2,32}$/.test(w))) return null;
  return `${cleaned.join("-")}${TUNNEL_SUFFIX}`;
}

export type RelinkResult = { ok: true; address: string } | { ok: false; why: OpenFailure };

export async function relink(
  store: KeyStore,
  address: string,
  env: Env,
  options: { preferPoll?: boolean; socketTimeoutMs?: number; handshakeMs?: number } = {},
): Promise<RelinkResult> {
  const device = await store.device();
  if (!device) return { ok: false, why: "refused" };
  try {
    const session = await Session.open({ address, device, env, ...options });
    session.close("relinked");
    const latest = (await store.device()) ?? device;
    await store.save({
      ...latest,
      address,
      computerName: session.hello.computerName,
      connection: session.hello.connection,
      version: session.hello.version,
      lastConnectedAt: env.now(),
    });
    return { ok: true, address };
  } catch (e) {
    if (e instanceof OpenError) {
      if (e.failure === "revoked") await store.clear();
      return { ok: false, why: e.failure };
    }
    throw e;
  }
}
