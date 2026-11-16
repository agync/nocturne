import type { CopyId } from "../../copy/t";
import type { IconName } from "../Icon/Icon";

interface ChipSpec {
  /** The copy key of the control itself, so a chip can never say a word the control does not. */
  copy: CopyId;
  /** Set only when the real control draws this icon beside its words. */
  icon?: IconName;
}

/**
 * Every place or control a sentence may name with a chip. A sentence names one by writing
 * `{id}` in its copy; the screen fills it with `tc()`.
 */
export const CHIPS = {
  settings: { copy: "rail.settings", icon: "settings" },
  privacy: { copy: "rail.privacy" },
  onlineAnswers: { copy: "online.row.label" },
  awayFromHome: { copy: "away.row.label" },
  voice: { copy: "settings.voice.label" },
  showAdvancedDetails: { copy: "settings.advanced.show" },
  connectOnlineService: { copy: "advanced.online.connect.link" },
  restoreFromBackup: { copy: "setup.welcome.restore" },
  seeWhatWasSent: { copy: "ask.mark.seeSent" },
  addAPhone: { copy: "away.phones.add" },
  newCodeToScan: { copy: "phone.address.scan" },
  phonesNeedNewAddress: { copy: "rail.away.relink" },
  allow: { copy: "away.pair.allow" },
  dontAllow: { copy: "away.pair.deny" },
} as const satisfies Record<string, ChipSpec>;

export type ChipId = keyof typeof CHIPS;

export const CHIP_IDS = Object.keys(CHIPS) as ChipId[];

export function chipSpec(id: ChipId): ChipSpec {
  return CHIPS[id];
}

/** Keys a sentence may name with a key cap, written as `{id}` in its copy. */
export const KEYS = {
  slash: "/",
} as const;

export type KeyId = keyof typeof KEYS;
