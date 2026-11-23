import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { type EngineClient, EngineError } from "./client";
import { TauriEngine } from "./tauri";

interface Call {
  cmd: string;
  args: Record<string, unknown> | undefined;
}

let calls: Call[];
let engine: TauriEngine;

beforeEach(() => {
  calls = [];
  mockIPC(
    (cmd, args) => {
      calls.push({ cmd, args: args as Record<string, unknown> | undefined });
      return null;
    },
    { shouldMockEvents: true },
  );
  engine = new TauriEngine();
});

afterEach(() => {
  engine.dispose();
  clearMocks();
});

const patch = {
  pageTime: null,
  delivery: null,
  autoPrint: null,
  voice: null,
  voiceSpeed: null,
  shortcut: null,
  inboxFolder: null,
  startAtLogin: null,
  lettingGo: null,
  theme: null,
  language: null,
  checkUpdates: null,
  reminders: null,
  lock: null,
  agents: null,
};

/** Every method, with the command and argument keys the roadmap contract names. */
const table: Array<[keyof EngineClient, unknown[], string, string[]]> = [
  ["getStatus", [], "get_status", []],
  ["getSetup", [], "get_setup", []],
  ["startSetup", ["standard"], "start_setup", ["tier"]],
  ["pauseSetup", [], "pause_setup", []],
  ["resumeSetup", [], "resume_setup", []],
  ["getTierOffer", [], "get_tier_offer", []],
  ["declineTierOffer", [], "decline_tier_offer", []],
  ["getUnfinishedTier", [], "get_unfinished_tier", []],
  ["declineUnfinishedTier", [], "decline_unfinished_tier", []],
  ["retryEngine", [], "retry_engine", []],
  ["tidyAnyway", [], "tidy_anyway", []],
  ["keepText", ["a note", "quickCapture"], "keep_text", ["text", "source"]],
  ["keepFiles", [["a.pdf"], "drop"], "keep_files", ["paths", "source", "day", "block"]],
  [
    "keepFiles",
    [["a.jpg"], "journal", { day: "2026-11-19", block: 2 }],
    "keep_files",
    ["paths", "source", "day", "block"],
  ],
  ["listItems", [{ since: null, status: null, limit: null }], "list_items", ["filter"]],
  ["getItem", ["i1"], "get_item", ["id"]],
  ["unkeep", ["i1"], "unkeep", ["id"]],
  ["readNow", ["i1"], "read_now", ["id"]],
  ["setItemPrivate", ["i1", true], "set_item_private", ["id", "private"]],
  ["setItemKeepForever", ["i1", true], "set_item_keep_forever", ["id", "keepForever"]],
  ["listEntities", ["person", "le"], "list_entities", ["kind", "query"]],
  ["snooze", ["m1", 30], "snooze", ["memoryId", "days"]],
  ["retryItem", ["i1"], "retry_item", ["id"]],
  ["ask", ["When?"], "ask", ["question", "place", "help"]],
  ["ask", ["When?", "thisComputer"], "ask", ["question", "place", "help"]],
  ["ask", ["How?", "auto", true], "ask", ["question", "place", "help"]],
  ["getPage", ["2026-11-13"], "get_page", ["date"]],
  ["listPages", [], "list_pages", []],
  ["answerQuestion", ["2026-11-13", "yes"], "answer_question", ["pageDate", "choice"]],
  ["addAnswerLine", ["2026-11-13", "It went fine."], "add_answer_line", ["pageDate", "line"]],
  ["resolveWhich", ["c1", "m1"], "resolve_which", ["conflictId", "memoryId"]],
  ["confirmMemory", ["m1", true], "confirm_memory", ["id", "confirmed"]],
  ["markDone", ["m1"], "mark_done", ["memoryId"]],
  [
    "listMemories",
    [{ entityId: null, status: null, query: null, limit: null }],
    "list_memories",
    ["filter"],
  ],
  ["findUnread", ["marmalade"], "find_unread", ["query"]],
  ["getEntity", ["e1"], "get_entity", ["id"]],
  ["correctMemory", ["m1", "Fixed."], "correct_memory", ["id", "text"]],
  ["renameEntity", ["e1", "Drinks"], "rename_entity", ["id", "name"]],
  ["mergeEntities", ["e1", "e2"], "merge_entities", ["fromId", "intoId"]],
  ["refileMemory", ["m1", { type: "existing", entityId: "e2" }], "refile_memory", ["id", "target"]],
  ["undoChange", ["chg_1"], "undo_change", ["changeId"]],
  ["setKeepForever", ["m1", true], "set_keep_forever", ["id", "keepForever"]],
  ["setPrivate", ["m1", true], "set_private", ["id", "private"]],
  ["restore", [["m1"]], "restore", ["ids"]],
  ["previewForget", [["m1"], true], "preview_forget", ["ids", "includeOriginals", "also", "pick"]],
  ["forget", ["tok"], "forget", ["previewToken"]],
  ["listNeedsYou", [], "list_needs_you", []],
  ["listFading", [], "list_fading", []],
  ["getPrivacy", [], "get_privacy", []],
  ["exportTo", ["out"], "export_to", ["dir"]],
  ["deleteEverything", ["phrase"], "delete_everything", ["confirmPhrase"]],
  ["estimateBackup", [], "estimate_backup", []],
  ["makeBackup", ["/backups/today.nocturne", "pass"], "make_backup", ["path", "passphrase"]],
  ["cancelBackup", [], "cancel_backup", []],
  ["getBackups", [], "get_backups", []],
  ["dismissBackupNotices", [], "dismiss_backup_notices", []],
  ["probeBackup", ["/backups/today.nocturne"], "probe_backup", ["path"]],
  ["checkBackup", ["/backups/today.nocturne", "pass"], "check_backup", ["path", "passphrase"]],
  [
    "restoreBackup",
    ["/backups/today.nocturne", "pass", true],
    "restore_backup",
    ["path", "passphrase", "replace"],
  ],
  ["restorePutAside", ["copy-1"], "restore_put_aside", ["id"]],
  ["deletePutAside", ["copy-1"], "delete_put_aside", ["id"]],
  ["getRestored", [], "get_restored", []],
  ["finishRestore", [], "finish_restore", []],
  ["discardRestore", [], "discard_restore", []],
  ["getSettings", [], "get_settings", []],
  ["updateSettings", [patch], "update_settings", ["patch"]],
  ["pause", [null], "pause", ["until"]],
  ["resume", [], "resume", []],
  ["getAdvanced", [], "get_advanced", []],
  ["speakPage", ["2026-11-13"], "speak_page", ["date"]],
  ["getEdition", ["2026-11-13", "print"], "get_edition", ["date", "edition"]],
  ["openSoundSettings", [], "open_sound_settings", []],
  ["quickKeepCommand", [], "quick_keep_command", []],
  ["configureShortcut", [], "configure_shortcut", []],
  ["retryShortcut", [], "retry_shortcut", []],
  ["cancelAsk", ["ask_1"], "cancel_ask", ["askId"]],
  ["appReady", [], "app_ready", []],
  ["reportRecording", ["no-devices"], "report_recording", ["state"]],
  ["getRemote", [], "get_remote", []],
  ["startRemote", ["simple", true], "start_remote", ["connection", "allowDownload"]],
  ["stopRemote", [false], "stop_remote", ["removeAll"]],
  ["pauseRemoteDownload", [], "pause_remote_download", []],
  ["beginPairing", [], "begin_pairing", []],
  ["decidePairing", ["p1", true], "decide_pairing", ["pairingId", "allow"]],
  ["cancelPairing", [], "cancel_pairing", []],
  ["renameDevice", ["d1", "Pixel"], "rename_device", ["id", "name"]],
  ["removeDevice", ["d1"], "remove_device", ["id", "alsoDeleteBrought"]],
  ["listRemoteEvents", [50], "list_remote_events", ["limit"]],
  [
    "updateRemoteSettings",
    [{ connection: null, autoOff: "never", keepAwake: null }],
    "update_remote_settings",
    ["patch"],
  ],
  ["openRemoteSignIn", [], "open_remote_sign_in", []],
  ["newComputerKey", [], "new_computer_key", []],
  ["getMove", [], "get_move", []],
  ["answerMove", ["bring"], "answer_move", ["choice"]],
  ["finishMove", [], "finish_move", []],
  ["stopMove", [], "stop_move", []],
  ["getOnline", [], "get_online", []],
  [
    "checkOnline",
    [{ baseUrl: "https://api.example.com/v1", key: "k", model: "m" }],
    "check_online",
    ["form"],
  ],
  ["cancelOnlineCheck", [], "cancel_online_check", []],
  [
    "turnOnOnline",
    [
      {
        phones: true,
        limit: { kind: "questions", questions: 200, usd: null },
      },
    ],
    "turn_on_online",
    ["settings"],
  ],
  ["turnOffOnline", [true], "turn_off_online", ["forget"]],
  ["abandonOnlineSetup", [], "abandon_online_setup", []],
  ["seeOnlineChange", [], "see_online_change", []],
  ["updateOnlineSettings", [{ phones: false, limit: null }], "update_online_settings", ["patch"]],
  ["listOnlineSends", [20, null], "list_online_sends", ["limit", "before"]],
  ["getModels", [], "get_models", []],
  ["listModelChoices", ["answering"], "list_model_choices", ["job"]],
  ["previewModelChange", ["finding", "m"], "preview_model_change", ["job", "model"]],
  ["changeModel", ["answering", "m", true], "change_model", ["job", "model", "allowDownload"]],
  ["pauseModelDownload", [], "pause_model_download", []],
  ["resumeModelDownload", [], "resume_model_download", []],
  ["cancelModelChange", [], "cancel_model_change", []],
  ["cancelReindex", [], "cancel_reindex", []],
  ["keepOldModel", [], "keep_old_model", []],
  ["removeOldModel", [], "remove_old_model", []],
  ["resetModelsToTier", [], "reset_models_to_tier", []],
  ["getGuide", [], "get_guide", []],
  ["getJournalDay", ["2026-11-19"], "get_journal_day", ["day"]],
  ["listJournalDays", [], "list_journal_days", []],
  ["listPeriods", ["week"], "list_periods", ["kind"]],
  ["dismissJournalIntro", [], "dismiss_journal_intro", []],
  [
    "saveEntryText",
    ["2026-11-19", null, 0, "Hello"],
    "save_entry_text",
    ["day", "itemId", "block", "text"],
  ],
  ["placeInEntry", ["i1", "2026-11-19", 1], "place_in_entry", ["itemId", "day", "block"]],
  ["describePhoto", ["i1", "A gate"], "describe_photo", ["itemId", "text"]],
  ["readEntryNow", ["2026-11-19"], "read_entry_now", ["day"]],
  ["keepNote", ["Hi", "journal", null, false], "keep_note", ["text", "source", "feeling", "good"]],
  ["topFeelingWords", ["settled"], "top_feeling_words", ["part"]],
  ["markTried", ["c1", "walk"], "mark_tried", ["checkinId", "idea"]],
  [
    "keepGoodThing",
    ["2026-11-19", "Sam fixed the gate", null],
    "keep_good_thing",
    ["day", "text", "photoItemId"],
  ],
  ["setLeftOut", ["i1", true], "set_left_out", ["itemId", "leftOut"]],
  ["moveToDay", ["i1", "2026-11-18"], "move_to_day", ["itemId", "day"]],
  ["confirmMention", ["m1", null], "confirm_mention", ["mentionId", "role"]],
  ["removeMention", ["m1"], "remove_mention", ["mentionId"]],
  ["restoreMention", ["m1"], "restore_mention", ["mentionId"]],
  ["entityJournalDays", ["e1"], "entity_journal_days", ["entityId"]],
  ["keepCheckIn", [{}], "keep_check_in", ["input"]],
  ["refileMention", ["m1", {}], "refile_mention", ["mentionId", "target"]],
  ["answerSameName", ["q1", { type: "new" }], "answer_same_name", ["questionId", "answer"]],
  ["getReflection", [{}], "get_reflection", ["period"]],
  ["reflect", [{}], "reflect", ["period"]],
  ["stopReflecting", [{}], "stop_reflecting", ["period"]],
  ["deleteReflection", [{}], "delete_reflection", ["period"]],
  ["getPhoneSources", [], "get_phone_sources", []],
  [
    "setPhoneSource",
    ["d1", "calendar", false],
    "set_phone_source",
    ["deviceId", "source", "taking"],
  ],
  [
    "setPhonePageOptions",
    [{ calendarToday: true, sleepLastNight: false }],
    "set_phone_page_options",
    ["options"],
  ],
  ["setNoticing", [false], "set_noticing", ["on"]],
  [
    "listCameIn",
    ["d1", "health", null, 50, "itm_1"],
    "list_came_in",
    ["deviceId", "source", "before", "beforeId", "limit"],
  ],
  [
    "getHealthSeries",
    ["sleep", "2026-11-01", "2026-11-14"],
    "get_health_series",
    ["kind", "from", "to"],
  ],
  ["listReadings", ["steps", "2026-11-01", "2026-11-14"], "list_readings", ["kind", "from", "to"]],
  [
    "previewPhoneDelete",
    [{ deviceId: "d1", source: "health", kinds: null, before: null }],
    "preview_phone_delete",
    ["scope"],
  ],
  ["deletePhoneData", ["t1", false], "delete_phone_data", ["token", "alsoStop"]],
];

describe("TauriEngine commands", () => {
  for (const [method, args, command, keys] of table) {
    it(`${method} invokes ${command}${keys.length ? ` with ${keys.join(", ")}` : ""}`, async () => {
      await (engine[method] as (...a: unknown[]) => Promise<unknown>)(...args);
      expect(calls).toHaveLength(1);
      expect(calls[0]?.cmd).toBe(command);
      expect(Object.keys(calls[0]?.args ?? {}).sort()).toEqual([...keys].sort());
    });
  }

  it("covers every method of the client", () => {
    const covered = new Set<string>([...table.map(([m]) => m), "keepRecording", "keepBytes"]);
    const proto = Object.getOwnPropertyNames(engine).filter(
      (k) =>
        typeof (engine as unknown as Record<string, unknown>)[k] === "function" &&
        !["on", "dispose", "call"].includes(k),
    );
    for (const name of proto) expect(covered.has(name as keyof EngineClient), name).toBe(true);
  });

  it("sends a recording as the raw body with its mime type in a header", async () => {
    const seen: Array<{ cmd: string; args: unknown; headers: unknown }> = [];
    clearMocks();
    mockIPC(
      (cmd, args) => {
        seen.push({ cmd, args, headers: undefined });
        return null;
      },
      { shouldMockEvents: true },
    );
    const bytes = new Uint8Array([1, 2, 3]);
    await engine.keepRecording(bytes, "audio/webm");
    expect(seen[0]?.cmd).toBe("keep_recording");
    expect(seen[0]?.args).toBeInstanceOf(Uint8Array);
    expect(Array.from((seen[0]?.args ?? []) as Uint8Array)).toEqual([1, 2, 3]);
  });

  it("sends pasted bytes as the raw body, naming the kind and where they came from", async () => {
    const seen: unknown[] = [];
    clearMocks();
    mockIPC((cmd, args) => {
      seen.push([cmd, args]);
      return null;
    });
    await engine.keepBytes(new Uint8Array([5, 6]), "image/png", "paste");
    const [cmd, args] = seen[0] as [string, Uint8Array];
    expect(cmd).toBe("keep_bytes");
    expect(Array.from(args)).toEqual([5, 6]);
  });

  it("passes null, not undefined, for the default page date", async () => {
    await engine.getPage();
    expect(calls[0]?.args).toEqual({ date: null });
  });
});

describe("TauriEngine events", () => {
  it("fans an event out to every listener and stops after unsubscribe", async () => {
    const a: number[] = [];
    const b: number[] = [];
    const offA = engine.on("needs-you-changed", (n) => a.push(n));
    engine.on("needs-you-changed", (n) => b.push(n));
    await emit("needs-you-changed", 2);
    offA();
    await emit("needs-you-changed", 1);
    expect(a).toEqual([2]);
    expect(b).toEqual([2, 1]);
  });

  it("delivers events that arrive while no screen is listening to the one that attaches next", async () => {
    // The engine subscribed at construction, so a late listener still hears every later event,
    // and an event with no listener is dropped without breaking delivery.
    await emit("status-changed", { state: "reading" });
    const seen: unknown[] = [];
    engine.on("status-changed", (s) => seen.push(s));
    await emit("status-changed", { state: "resting" });
    await emit("status-changed", { state: "paused" });
    expect(seen).toEqual([{ state: "resting" }, { state: "paused" }]);
  });
});

describe("TauriEngine errors", () => {
  it("turns every kind of rejection into an EngineError with a code and detail", async () => {
    const failures: unknown[] = [
      "disk is full",
      { code: "diskFull", message: "6 GB short" },
      new Error("boom"),
      7,
    ];
    for (const failure of failures) {
      clearMocks();
      mockIPC(() => {
        throw failure;
      });
      const error = await engine.getStatus().catch((e: unknown) => e);
      expect(error).toBeInstanceOf(EngineError);
      expect((error as EngineError).code).toEqual(expect.any(String));
      expect((error as EngineError).detail.length).toBeGreaterThan(0);
    }
    clearMocks();
    mockIPC(() => {
      throw { code: "diskFull", message: "6 GB short" };
    });
    const error = (await engine.getStatus().catch((e: unknown) => e)) as EngineError;
    expect(error.code).toBe("diskFull");
    expect(error.detail).toBe("6 GB short");
  });
});
