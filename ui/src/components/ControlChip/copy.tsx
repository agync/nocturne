import { Children, type ReactNode } from "react";
import { type CopyId, type CopyValues, t, tParts } from "../../copy/t";
import { KeyCap } from "../KeyCap/KeyCap";
import { ControlChip } from "./ControlChip";
import { CHIP_IDS, CHIPS, KEYS, type KeyId } from "./registry";

const KEY_IDS = Object.keys(KEYS) as KeyId[];

const elements: Record<string, ReactNode> = {
  ...Object.fromEntries(CHIP_IDS.map((id) => [id, <ControlChip key={id} id={id} />])),
  ...Object.fromEntries(KEY_IDS.map((id) => [id, <KeyCap key={id}>{KEYS[id]}</KeyCap>])),
};

/** A sentence that names places and keys: each `{settings}` or `{slash}` in it is drawn as a chip or key cap. */
export function tc(id: CopyId, values?: CopyValues): ReactNode {
  return Children.toArray(tParts(id, { ...elements, ...values }) as ReactNode[]);
}

/** The same sentence as plain text, as a screen reader, a copy and a notification would have it. */
export function plain(id: CopyId, values?: CopyValues): string {
  const words = {
    ...Object.fromEntries(CHIP_IDS.map((chip) => [chip, t(CHIPS[chip].copy)])),
    ...KEYS,
  };
  return t(id, { ...words, ...values });
}
