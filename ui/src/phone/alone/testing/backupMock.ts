// The nightly backup as the scripted phone answers it. `&b=` names how it stands: `off` (the
// default), `on`, `wifi`, `noRoom`, `signedOut`, `noPermission`, `another`, `newer`, `kept`,
// `late`, `notCharged`, `forgot`, `deleting`, `moved`, or with no backup yet `none`,
// `noRoomNone`, `otherBackup` or `needsPhrase`. `&found=none` finds no backup,
// `&found=offline` no internet, `&connect=` what asking for the account answers (`refused`,
// `noAccount`, `turnedOff`, `offline`, `full`), `&delete=offline` keeps a deletion waiting, `&key=offline` keeps a new key from the
// account until `scriptedBackup.wifi`,
// `&restore=slow` holds a restore part way until it is stopped (`&checking` once it checks,
// `&adding` once it is checked), `&restore=newer` is a newer Nocturne's, `&own` is this phone's own,
// `&restore=offline` loses the internet until
// `scriptedBackup.wifi`, and `&net=mobile` is on mobile data until `scriptedBackup.wifi`.
// The phrase "wrong words" and any recovery key but the one below open nothing.

import { emit } from "@tauri-apps/api/event";
import type { BackupStatus, Connection, NightView, Restored } from "../nightly";

export const SCRIPTED_PHRASE = "harbour violet pencil maple drift oven";
export const SCRIPTED_KEY = "K7QM-2XWD-9HRT-P4NC-B8LZ-6VFJ";
export const NEW_KEY = "M3TD-7KWQ-4HXN-R9CP-2BZL-8VFG";

const GB = 1_000_000_000;
const MB = 1_000_000;
/** The scripted clock reads 13 November, 14:05. */
const at = (day: number, time: string) => `2026-11-${String(day).padStart(2, "0")}T${time}:00Z`;

/** The kept things grow by twenty each night from 1,980. */
const night = (count: number, madeAt: string, bytes: number): NightView => ({
  count,
  madeAt,
  bytes,
  items: 1980 + count * 20,
  first: "2025-03-02T09:14:00Z",
});

const NIGHTS: NightView[] = [
  night(7, at(13, "03:12"), 1.4 * GB),
  night(6, at(12, "02:58"), 1.4 * GB),
  night(5, at(11, "02:40"), 1.4 * GB),
  night(4, at(10, "03:21"), 1.3 * GB),
  night(3, at(9, "03:05"), 1.3 * GB),
  night(2, at(8, "02:47"), 1.3 * GB),
  night(1, at(7, "03:30"), 1.3 * GB),
];

/** Off, with nothing kept. */
const OFF: BackupStatus = {
  on: false,
  wifiOnly: false,
  last: null,
  state: { state: "off" },
  moved: false,
  deletePending: false,
  forgetWaiting: false,
};

function first(): BackupStatus {
  const b = new URLSearchParams(location.search).get("b") ?? "off";
  const last = { at: at(13, "03:12"), bytes: 1.4 * GB };
  const stopped = { at: at(9, "03:05"), bytes: 1.3 * GB };
  const on = (state: BackupStatus["state"], l: BackupStatus["last"] = last): BackupStatus => ({
    ...OFF,
    on: true,
    last: l,
    state,
  });
  switch (b) {
    case "on":
      return on({ state: "upToDate" });
    case "wifi":
      return {
        ...on({ state: "waitingForWifi" }, { at: at(11, "02:40"), bytes: 1.4 * GB }),
        wifiOnly: true,
      };
    case "noRoom":
      return on({ state: "noRoom" }, stopped);
    case "signedOut":
      return on({ state: "signedOut" }, stopped);
    case "noPermission":
      return on({ state: "noPermission" }, stopped);
    case "another":
      return on({ state: "anotherPhone", madeAt: at(12, "22:10") }, stopped);
    case "newer":
      return on({ state: "newer" }, stopped);
    case "otherBackup":
      return on({ state: "otherBackup" }, null);
    case "needsPhrase":
      return on({ state: "needsPhrase" }, null);
    case "noRoomNone":
      return on({ state: "noRoom" }, null);
    case "none":
      return on({ state: "waiting" }, null);
    case "late":
      return on({ state: "waiting" }, { at: at(4, "03:00"), bytes: 1.3 * GB });
    case "notCharged":
      return on({ state: "notCharged" }, { at: at(11, "02:40"), bytes: 1.4 * GB });
    case "forgot":
      return { ...on({ state: "waiting" }), forgetWaiting: true };
    case "deleting":
      return { ...OFF, deletePending: true };
    case "kept":
      return { ...OFF, last: stopped };
    case "moved":
      return { ...OFF, last: stopped, moved: true };
    default:
      return OFF;
  }
}

/** The scripted backup's state, and what each call has been told, for the tests to read. */
export const scriptedBackup = {
  calls: [] as string[],
  phrase: null as string | null,
  key: null as string | null,
  /** With `&net=mobile`, whether the phone has come onto Wi-Fi; with `&restore=offline`,
   * whether the internet came back. */
  wifi: false,
  /** What "Copy" put on the phone's own clipboard. */
  copied: null as string | null,
};

export function backupState(phone: {
  /** The phone holds a memory, so the backup's things join it. */
  holds(): boolean;
  /** An empty phone now holds the backup's memory. */
  restored(): void;
}) {
  const params = new URLSearchParams(location.search);
  let status = first();
  let phrase: string | null = status.on || status.last ? SCRIPTED_PHRASE : null;
  let key: string | null = status.last ? SCRIPTED_KEY : null;
  // A key shown but not yet saved: it opens nothing.
  let shown: string | null = null;
  let stopRestore: (() => void) | null = null;
  scriptedBackup.calls = [];
  scriptedBackup.wifi = false;
  scriptedBackup.copied = null;

  const opens = (opening: { with: string; text: string }) => {
    const text = opening.text.trim().toLowerCase().replace(/\s+/g, " ");
    if (opening.with === "recoveryKey")
      return text.toUpperCase().replace(/[\s-]/g, "") === SCRIPTED_KEY.replace(/-/g, "");
    return text === SCRIPTED_PHRASE;
  };

  const progress = (phase: "restoring" | "checking", done: number, total: number) =>
    void emit("backup-progress", { phase, done, total });

  function answer(cmd: string, args: Record<string, unknown>): unknown {
    if (cmd.startsWith("backup_")) scriptedBackup.calls.push(cmd);
    switch (cmd) {
      case "backup_status":
        return status;
      case "backup_new_phrase":
        return scriptedBackup.calls.filter((c) => c === "backup_new_phrase").length > 1
          ? "copper lantern meadow ribbon quiet anchor"
          : SCRIPTED_PHRASE;
      case "backup_connect": {
        const c = params.get("connect");
        const answer: Connection =
          c === "refused" || c === "noAccount" || c === "turnedOff" || c === "offline"
            ? { outcome: c }
            : {
                outcome: "connected",
                account: "maya.k@example.com",
                previous: args.choose ? "maya.old@example.com" : null,
                free: c === "full" ? 300 * MB : 12 * GB,
              };
        // Signed in again, or allowed again: the stop is over.
        if (
          answer.outcome === "connected" &&
          (status.state.state === "signedOut" || status.state.state === "noPermission")
        )
          status = { ...status, state: { state: "upToDate" } };
        return answer;
      }
      case "backup_turn_on":
        phrase = String(args.phrase);
        scriptedBackup.phrase = phrase;
        status = { ...status, on: true, last: null, state: { state: "waiting" } };
        return null;
      case "backup_recovery_key":
        shown = key === null ? SCRIPTED_KEY : NEW_KEY;
        return shown;
      case "backup_keep_recovery_key":
        if (shown === null) throw "notReady";
        if (params.get("key") === "offline" && !scriptedBackup.wifi) throw "offline";
        key = shown;
        shown = null;
        scriptedBackup.key = key;
        return null;
      case "backup_drop_recovery_key":
        shown = null;
        return null;
      case "backup_change_phrase":
        phrase = String(args.phrase);
        scriptedBackup.phrase = phrase;
        return null;
      case "backup_set_wifi_only":
        status = { ...status, wifiOnly: Boolean(args.on) };
        return null;
      case "backup_now": {
        if (status.wifiOnly && !args.overMobileData) throw "waitingForWifi";
        const total = 1.4 * GB;
        void emit("backup-progress", { phase: "saving", done: 0.54 * GB, total });
        if (params.get("first") === "slow") return new Promise(() => undefined);
        const last = { at: at(13, "21:52"), bytes: total };
        status = { ...status, last, state: { state: "upToDate" } };
        return last;
      }
      case "backup_turn_off":
        status = args.alsoDelete
          ? { ...OFF, wifiOnly: status.wifiOnly, deletePending: params.get("delete") === "offline" }
          : { ...status, on: false, state: { state: "off" } };
        if (args.alsoDelete && status.deletePending) throw "offline";
        return null;
      case "backup_turn_on_again":
        if (!status.last && !status.moved && status.state.state !== "notInUse") throw "off";
        status = { ...status, on: true, state: { state: "upToDate" } };
        return null;
      case "backup_found": {
        const f = params.get("found");
        if (f === "none") return null;
        if (f === "offline") throw "offline";
        if (f === "icloudOff") throw "noPermission";
        return { at: NIGHTS[0]?.madeAt ?? null, bytes: 1.4 * GB };
      }
      case "backup_nights":
        // Before the phrase, only what the account's listing says.
        if (!args.opening) return NIGHTS.map((n) => ({ ...n, items: null, first: null }));
        if (!opens(args.opening as { with: string; text: string })) throw "wrongPhrase";
        return NIGHTS;
      case "backup_here":
        return { things: 412, first: "2026-06-20T08:00:00Z" };
      case "backup_on_wifi":
        return params.get("net") === "mobile" ? scriptedBackup.wifi : true;
      case "backup_restore": {
        if (!opens(args.opening as { with: string; text: string })) throw "wrongPhrase";
        if (params.get("restore") === "missing" && args.night === null) throw "damaged";
        if (params.get("restore") === "newer") throw { newer: { theirs: "0.9.0", ours: "0.8.0" } };
        if (params.get("restore") === "offline" && !scriptedBackup.wifi) throw "offline";
        progress("restoring", 650 * MB, 1.4 * GB);
        const summary = {
          madeAt: NIGHTS[0]?.madeAt ?? "",
          appVersion: "0.8.0",
          storeVersion: 1,
          memories: 0,
          items: 2140,
          pages: 120,
          originals: 2140,
          onlineUsed: true,
          awayUsed: false,
          bytes: 1.4 * GB,
        };
        const done: Restored = phone.holds()
          ? {
              kind: "broughtIn",
              summary,
              things: 1900,
              entries: 240,
              checkins: 118,
              same: 12,
              already: 0,
              own: params.has("own"),
            }
          : {
              kind: "replaced",
              summary,
              kept: { things: 2140, journalEntries: 300, photosAndRecordings: 410, checkins: 140 },
              own: params.has("own"),
            };
        if (params.get("restore") === "slow")
          return new Promise((resolve, reject) => {
            stopRestore = () => reject("stopped");
            if (params.has("checking")) progress("checking", 0.4 * GB, 1.4 * GB);
            if (params.has("adding")) progress("checking", 1.4 * GB, 1.4 * GB);
            void resolve;
          });
        status = OFF;
        if (done.kind === "replaced") phone.restored();
        return done;
      }
      case "backup_make_in_use":
        status = {
          ...status,
          on: true,
          last: { at: at(13, "03:12"), bytes: 1.4 * GB },
          state: { state: "upToDate" },
        };
        return null;
      case "backup_stop":
        stopRestore?.();
        stopRestore = null;
        return null;
      case "backup_copy":
        scriptedBackup.copied = String(args.text);
        return true;
      case "backup_save_key":
        return "saved";
      default:
        return undefined;
    }
  }

  return { answer, phrase: () => phrase };
}
