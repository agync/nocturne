import { type ComputerKind, computerKind } from "../screens/away/os";

const COMMAND_OR_CONTROL = /^(command|cmd)or(control|ctrl)$/i;
const COMMAND = /^(command|cmd|super)$/i;
const ALT = /^(alt|option)$/i;

/** The keys of a shortcut as they are named on this computer's keyboard. */
export function shortcutKeys(shortcut: string, kind: ComputerKind = computerKind()): string[] {
  return shortcut.split("+").map((raw) => {
    const key = raw.trim();
    if (COMMAND_OR_CONTROL.test(key)) return kind === "mac" ? "Cmd" : "Ctrl";
    if (COMMAND.test(key)) return kind === "mac" ? "Cmd" : kind === "windows" ? "Win" : "Super";
    if (ALT.test(key)) return kind === "mac" ? "Option" : "Alt";
    return /^control$/i.test(key) ? "Ctrl" : key;
  });
}

/** "CommandOrControl+Shift+Space" as a person reads it: "Ctrl Shift Space", or "Cmd Shift Space" on a Mac. */
export function shortcutLabel(shortcut: string, kind?: ComputerKind): string {
  return shortcutKeys(shortcut, kind).join(" ");
}
