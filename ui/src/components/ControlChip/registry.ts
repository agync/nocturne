import type { CopyId } from "../../copy/t";
import type { IconName } from "../Icon/Icon";
import chips from "./chips.json";

interface ChipSpec {
  /** The copy key of the control itself, so a chip can never say a word the control does not. */
  copy: CopyId;
  /** Set only when the real control draws this icon beside its words. */
  icon?: IconName;
}

/**
 * Every place or control a sentence or the Nocturne guide may name with a chip. A sentence names
 * one by writing `{id}` in its copy; the screen fills it with `tc()`. The guide names one in bold,
 * in its own words, and the app's guide bundle (`src-tauri/src/help.rs`) reads this same file, so
 * one list holds every chip.
 */
export const CHIPS = chips as Record<keyof typeof chips, ChipSpec>;

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
