import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useId, useRef, useState } from "react";
import { formatBytes, formatLongDay } from "../../../copy/format";
import { t } from "../../../copy/t";
import type { BackupProgress } from "../../../engine/bindings";
import { Group, Row } from "../../native/screens/chrome";
import { Switch, useHeading } from "../../native/screens/parts";
import { APP_STORE, STORE } from "../../native/screens/UpdateCard";
import { openAppSettings } from "../../native/sources";
import { BottomSheet } from "../../screens/Dialog";
import { BackBar, Button } from "../../screens/ui";
import {
  type BackupApi,
  type BackupStatus,
  backupDay,
  backupTime,
  KEY_MADE,
  keptOff,
  onPhone,
  problemOf,
  stopCause,
} from "../nightly";
import type { NoteStore } from "../reminder";
import { AboutLines, KeyShown } from "./NightlyOn";

/** Google Drive, where an Android phone's backup is. */
const DRIVE = "https://drive.google.com/";

const open = (url: string) => void invoke("plugin:opener|open_url", { url }).catch(() => undefined);

/** The hint under "Back up every night". */
export function backupHint(
  s: BackupStatus,
  ios: boolean,
  progress: BackupProgress | null,
  now = new Date(),
): string | undefined {
  const running = progress && progress.total > 0 && progress.done < progress.total;
  if (running) {
    const values = { done: formatBytes(progress.done), size: formatBytes(progress.total) };
    return s.last ? t("phone.backup.hint.running", values) : t("phone.backup.hint.first", values);
  }
  if (!s.on || s.state.state === "notInUse") {
    if (!keptOff(s) || !s.last) return t("phone.backup.hint.off");
    return onPhone(ios, "phone.backup.hint.offKept.ios", "phone.backup.hint.offKept.android", {
      day: backupDay(s.last.at, now),
    });
  }
  // The first night is under way; how far it is shows once it says.
  if (!s.last) return undefined;
  const day = backupDay(s.last.at, now);
  const time = backupTime(s.last.at);
  switch (s.state.state) {
    case "upToDate":
    case "running":
      return onPhone(ios, "phone.backup.hint.on.ios", "phone.backup.hint.on.android", {
        day,
        time,
        size: formatBytes(s.last.bytes),
      });
    case "waitingForWifi":
      return t("phone.backup.hint.wifi", { day, time });
    case "noRoom":
    case "signedOut":
    case "noPermission":
    case "anotherPhone":
    case "newer":
      return t("phone.backup.hint.stopped", { day, time });
    default:
      // Waiting for tonight, no internet, or a night that did not finish: it carries on.
      return t("phone.backup.hint.tonight", { day, time });
  }
}

/** The account did not answer: the backup is off, and goes from the account once it does. */
const PENDING: ReadonlySet<string> = new Set([
  "offline",
  "unavailable",
  "signedOut",
  "noPermission",
]);

type Sheet = "off" | "makeKey" | "change" | "inUse" | null;

/**
 * Backup in This phone: the switch and how it stands, Wi-Fi only, Back up now, the phrase, a
 * recovery key and turning it off, then what to know about it.
 */
export function NightlyBackup({
  api,
  ios,
  notes,
  now,
  onBack,
  onTurnOn,
  onChangePhrase,
}: {
  api: BackupApi;
  ios: boolean;
  notes: NoteStore;
  now(): number;
  onBack(): void;
  /** The three steps, for a backup that was never on here or was deleted. */
  onTurnOn(): void;
  /** Steps 2 and 3, after "Change it". */
  onChangePhrase(): void;
}) {
  const [status, setStatus] = useState<BackupStatus | null>(null);
  const [progress, setProgress] = useState<BackupProgress | null>(null);
  const [sheet, setSheet] = useState<Sheet>(null);
  const [alsoDelete, setAlsoDelete] = useState(false);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const [onAgain, setOnAgain] = useState(false);
  const [deleted, setDeleted] = useState(false);
  const [keyMade, setKeyMade] = useState<string | null>(null);
  const [key, setKey] = useState<string | null>(null);
  const [account, setAccount] = useState<string | null>(null);
  const heading = useHeading("backup");
  const tick = useId();

  const look = useCallback(
    () =>
      api.status().then(
        (s) => {
          setStatus(s);
          return s;
        },
        () => null,
      ),
    [api],
  );
  // Back from Settings, an iPhone asks iCloud again, so signing in or turning iCloud on for
  // Nocturne lets the backup carry on with nothing more to press.
  const stopped = useRef(false);
  stopped.current = status?.state.state === "signedOut" || status?.state.state === "noPermission";
  useEffect(() => {
    void look();
    void notes.get(KEY_MADE).then(
      (v) => setKeyMade(v || null),
      () => undefined,
    );
    const front = () => {
      if (document.visibilityState !== "visible") return;
      if (ios && stopped.current)
        void api
          .connect(false)
          .catch(() => undefined)
          .then(() => look());
      else void look();
    };
    document.addEventListener("visibilitychange", front);
    return () => document.removeEventListener("visibilitychange", front);
  }, [look, notes, api, ios]);

  // A night running in the background says how far it is, and the hint moves on when it ends.
  useEffect(
    () =>
      api.watch((p) => {
        setProgress(p);
        if (p.total > 0 && p.done >= p.total) void look();
      }),
    [api, look],
  );

  const run = async (job: () => Promise<unknown>) => {
    if (busy) return;
    setBusy(true);
    setFailed(false);
    try {
      await job();
      setSheet(null);
    } catch {
      setFailed(true);
    } finally {
      setBusy(false);
      await look();
    }
  };

  const backUpNow = (overMobileData: boolean) =>
    void run(async () => {
      try {
        await api.now(overMobileData, setProgress);
      } catch (e) {
        // Waiting for Wi-Fi says so in its own line.
        if (problemOf(e) !== "waitingForWifi") throw e;
      } finally {
        setProgress(null);
      }
    });

  const turnOn = () =>
    void run(async () => {
      try {
        await api.turnOnAgain();
      } catch (e) {
        // Never on here, or its key went with the backup: the three steps. Anything else says so.
        if (problemOf(e) === "off") return onTurnOn();
        throw e;
      }
      const s = await api.status();
      if (s.state.state === "notInUse") await api.makeInUse();
      setDeleted(false);
      setOnAgain(true);
    });

  const turnOff = () =>
    void run(async () => {
      await api.turnOff(alsoDelete).catch((e: unknown) => {
        // Off already; the backup in the account goes once it answers.
        if (!alsoDelete || !PENDING.has(problemOf(e))) throw e;
      });
      setOnAgain(false);
      setDeleted(alsoDelete);
      setAlsoDelete(false);
    });

  const makeKey = () =>
    void run(async () => {
      setKey(await api.recoveryKey());
    });

  const chooseAccount = () =>
    void run(async () => {
      const c = await api.connect(true);
      if (c.outcome !== "connected") return;
      if (c.previous) setAccount(c.previous);
      // A night written clears the stop; until then nothing runs.
      await api.now(false, setProgress).catch(() => undefined);
      setProgress(null);
    });

  if (key)
    return (
      <KeyShown
        text={key}
        later
        save={api.saveKey}
        onSaved={() => {
          const at = new Date(now()).toISOString();
          setKeyMade(at);
          void notes.set(KEY_MADE, at).catch(() => undefined);
        }}
        onClose={(saved) => {
          // A key that was never saved must not open the backup.
          if (!saved) {
            void api.dropRecoveryKey().catch(() => undefined);
            // The earlier key stopped working when this one was made.
            setKeyMade(null);
            void notes.set(KEY_MADE, "").catch(() => undefined);
          }
          setKey(null);
        }}
      />
    );

  const s = status;
  const state = s?.state.state;
  const notInUse = state === "notInUse";
  const switchOn = Boolean(s?.on) && !notInUse;
  const day = s?.last ? backupDay(s.last.at, new Date(now())) : "";
  const madeAt = s?.state.state === "anotherPhone" ? formatLongDay(s.state.madeAt) : "";

  let say: {
    line: string;
    action: string;
    onAction(): void;
    calm?: boolean;
    /** Where to go in Settings, which opens on Nocturne's own page. */
    way?: string;
  } | null = null;
  if (s?.on) {
    if (state === "waitingForWifi")
      say = {
        line: t("phone.backup.say.wifi"),
        action: t("phone.backup.say.wifi.action"),
        onAction: () => backUpNow(true),
        calm: true,
      };
    else if (state === "noRoom")
      say = {
        line: onPhone(ios, "phone.backup.say.noRoom.ios", "phone.backup.say.noRoom.android", {
          day,
        }),
        action: onPhone(
          ios,
          "phone.backup.say.noRoom.action.ios",
          "phone.backup.say.noRoom.action.android",
        ),
        onAction: () => (ios ? void openAppSettings() : open(DRIVE)),
        way: ios ? t("phone.backup.say.settingsWay.icloud") : undefined,
      };
    else if (state === "signedOut")
      say = {
        line: onPhone(ios, "phone.backup.say.signedOut.ios", "phone.backup.say.signedOut.android"),
        action: ios ? t("phone.backup.say.openSettings") : t("phone.backup.say.chooseAccount"),
        onAction: () => (ios ? void openAppSettings() : chooseAccount()),
        way: ios ? t("phone.backup.say.settingsWay.signIn") : undefined,
      };
    else if (state === "noPermission")
      say = {
        line: onPhone(
          ios,
          "phone.backup.say.noPermission.ios",
          "phone.backup.say.noPermission.android",
        ),
        action: ios ? t("phone.backup.say.openSettings") : t("phone.backup.say.chooseAccount"),
        onAction: () => (ios ? void openAppSettings() : chooseAccount()),
        way: ios ? t("phone.backup.say.settingsWay.icloud") : undefined,
      };
    else if (state === "anotherPhone")
      say = {
        line: t("phone.backup.say.another", { day: madeAt }),
        action: t("phone.backup.say.another.action"),
        onAction: () => setSheet("inUse"),
      };
    else if (state === "newer")
      say = {
        line: t("phone.backup.say.newer", {
          store: onPhone(ios, "phone.backup.store.ios", "phone.backup.store.android"),
        }),
        action: t("phone.backup.say.newer.action"),
        onAction: () => open(ios ? APP_STORE : STORE),
      };
  }

  const hint = s ? backupHint(s, ios, progress, new Date(now())) : undefined;
  const running = progress !== null && progress.total > 0 && progress.done < progress.total;
  return (
    <div className="ph-screen">
      <BackBar title={t("phone.backup.bar")} onBack={onBack} label={t("phone.sheet.title")} />
      <main className="ph-main ph-settings">
        <h2 className="sr-only" tabIndex={-1} ref={heading}>
          {t("phone.backup.bar")}
        </h2>
        {s ? (
          <section className="ph-sgroup">
            <div className="ph-srows">
              <Switch
                label={t("phone.backup.switch")}
                hint={hint}
                on={switchOn}
                disabled={busy}
                onChange={(on) => (on ? turnOn() : setSheet("off"))}
              />
            </div>
            {onAgain && switchOn ? (
              <p className="ph-srow-hint ph-nb-under" role="status">
                {t("phone.backup.hint.onAgain")}
              </p>
            ) : null}
            {deleted && !s.on ? (
              <p className="ph-srow-hint ph-nb-under" role="status">
                {t("phone.backup.hint.deleted")}
              </p>
            ) : null}
            {say ? (
              <div className={say.calm ? "ph-nb-say ph-nb-say--calm" : "ph-nb-say"} role="status">
                <span>{say.line}</span>
                {say.way ? <span className="ph-small">{say.way}</span> : null}
                <button type="button" className="ph-link" onClick={say.onAction}>
                  {say.action}
                </button>
              </div>
            ) : null}
            {account ? (
              <p className="ph-srow-hint ph-nb-under" role="status">
                {t("phone.backup.say.anotherAccount", { account })}
              </p>
            ) : null}
          </section>
        ) : null}
        {switchOn ? (
          <Group>
            <Switch
              label={t("phone.backup.wifi")}
              on={Boolean(s?.wifiOnly)}
              onChange={(on) => void run(() => api.setWifiOnly(on))}
            />
            <Row
              label={
                running
                  ? t("phone.backup.hint.running", {
                      done: formatBytes(progress.done),
                      size: formatBytes(progress.total),
                    })
                  : t("phone.backup.now")
              }
              onPress={running || busy ? undefined : () => backUpNow(false)}
              action
            />
            <Row label={t("phone.backup.change")} onPress={() => setSheet("change")} />
            <Row
              label={t("phone.backup.makeKeyRow")}
              hint={
                keyMade
                  ? t("phone.backup.makeKeyRow.made", { day: backupDay(keyMade, new Date(now())) })
                  : t("phone.backup.makeKeyRow.none")
              }
              onPress={() => setSheet("makeKey")}
            />
            <Row label={t("phone.backup.turnOff")} action onPress={() => setSheet("off")} />
          </Group>
        ) : null}
        {failed && sheet === null ? (
          <p className="ph-error" role="alert">
            {t("ask.action.failed")}
          </p>
        ) : null}
        <section className="ph-sgroup" aria-labelledby="ph-nb-about">
          <h2 className="ph-sghead" id="ph-nb-about">
            {t("phone.backup.about.heading")}
          </h2>
          <div className="ph-nb-about">
            <AboutLines ios={ios} all={false} />
          </div>
        </section>
      </main>
      {sheet === "off" ? (
        <BottomSheet label={t("phone.backup.off.title")} onClose={() => setSheet(null)}>
          <div className="ph-sheet-body ph-stack" style={{ gap: 14 }}>
            <h2 className="ph-h1 ph-h1--small">{t("phone.backup.off.title")}</h2>
            <p className="ph-body ph-ink2">
              {onPhone(ios, "phone.backup.off.body.ios", "phone.backup.off.body.android")}
            </p>
            <label className="ph-nb-tick" htmlFor={tick}>
              <input
                id={tick}
                type="checkbox"
                checked={alsoDelete}
                onChange={(e) => setAlsoDelete(e.target.checked)}
              />
              <span>{t("phone.backup.off.delete")}</span>
            </label>
            {alsoDelete ? (
              <p className="ph-small">
                {onPhone(
                  ios,
                  "phone.backup.off.deleteHint.ios",
                  "phone.backup.off.deleteHint.android",
                )}
              </p>
            ) : null}
            {failed ? (
              <p className="ph-error" role="alert">
                {t("ask.action.failed")}
              </p>
            ) : null}
            <Button kind={alsoDelete ? "danger" : "primary"} disabled={busy} onClick={turnOff}>
              {t("phone.backup.off.yes")}
            </Button>
            <button type="button" className="ph-textbtn" onClick={() => setSheet(null)}>
              {t("phone.backup.off.no")}
            </button>
          </div>
        </BottomSheet>
      ) : null}
      {sheet === "makeKey" ? (
        <Ask
          title={t("phone.backup.makeKey.title")}
          body={t("phone.backup.makeKey.body")}
          yes={t("phone.backup.makeKey.yes")}
          busy={busy}
          failed={failed}
          onYes={makeKey}
          onNo={() => setSheet(null)}
        />
      ) : null}
      {sheet === "change" ? (
        <Ask
          title={t("phone.backup.changeAsk.title")}
          body={t("phone.backup.changeAsk.body")}
          yes={t("phone.backup.changeAsk.yes")}
          busy={busy}
          failed={false}
          onYes={() => {
            setSheet(null);
            onChangePhrase();
          }}
          onNo={() => setSheet(null)}
        />
      ) : null}
      {sheet === "inUse" ? (
        <Ask
          title={t("phone.backup.inUse.title")}
          body={t("phone.backup.inUse.body")}
          yes={t("phone.backup.inUse.yes")}
          busy={busy}
          failed={failed}
          onYes={() => void run(() => api.makeInUse())}
          onNo={() => setSheet(null)}
        />
      ) : null}
    </div>
  );
}

/** A question in a sheet: its title, one line, and two answers. */
function Ask({
  title,
  body,
  yes,
  busy,
  failed,
  onYes,
  onNo,
}: {
  title: string;
  body: string;
  yes: string;
  busy: boolean;
  failed: boolean;
  onYes(): void;
  onNo(): void;
}) {
  return (
    <BottomSheet label={title} onClose={onNo}>
      <div className="ph-sheet-body ph-stack" style={{ gap: 14 }}>
        <h2 className="ph-h1 ph-h1--small">{title}</h2>
        <p className="ph-body ph-ink2">{body}</p>
        {failed ? (
          <p className="ph-error" role="alert">
            {t("ask.action.failed")}
          </p>
        ) : null}
        <Button kind="primary" disabled={busy} onClick={onYes}>
          {yes}
        </Button>
        <button type="button" className="ph-textbtn" onClick={onNo}>
          {t("phone.backup.cancel")}
        </button>
      </div>
    </BottomSheet>
  );
}

/** The row under Your memory in This phone: "Off", or how the backup stands. */
export function backupRowHint(s: BackupStatus | null, ios: boolean, now: Date): string | undefined {
  if (!s) return undefined;
  if (!s.on) return t("phone.backup.row.off");
  return backupHint(s, ios, null, now);
}

/** The Today card for a backup that stopped, or went seven nights without one; null for none. */
export function todayCard(
  s: BackupStatus,
  ios: boolean,
  now: Date,
): { cause: string; stopped: boolean; line: string } | null {
  const cause = stopCause(s);
  const day = s.last ? backupDay(s.last.at, now) : "";
  const at = s.last?.at ?? "";
  if (cause) {
    const line = {
      noRoom: onPhone(ios, "phone.backup.say.noRoom.ios", "phone.backup.say.noRoom.android", {
        day,
      }),
      signedOut: onPhone(
        ios,
        "phone.backup.say.signedOut.ios",
        "phone.backup.say.signedOut.android",
      ),
      noPermission: onPhone(
        ios,
        "phone.backup.say.noPermission.ios",
        "phone.backup.say.noPermission.android",
      ),
      anotherPhone: t("phone.backup.say.another", {
        day: s.state.state === "anotherPhone" ? formatLongDay(s.state.madeAt) : "",
      }),
      newer: t("phone.backup.say.newer", {
        store: onPhone(ios, "phone.backup.store.ios", "phone.backup.store.android"),
      }),
    }[cause];
    return { cause: `${cause}:${at}`, stopped: true, line };
  }
  if (s.on && s.last && now.getTime() - new Date(s.last.at).getTime() >= SEVEN_NIGHTS_MS)
    return {
      cause: `late:${at}`,
      stopped: false,
      line: t("phone.backup.card.last", { day: backupDay(s.last.at, now) }),
    };
  return null;
}

const SEVEN_NIGHTS_MS = 7 * 86_400_000;

/**
 * On a phone whose memory moved to a computer: a backup kept in the account, there only to be
 * deleted.
 */
export function MovedBackup({ api, ios }: { api: BackupApi; ios: boolean }) {
  const [status, setStatus] = useState<BackupStatus | null>(null);
  const [asking, setAsking] = useState(false);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    void api.status().then(setStatus, () => undefined);
  }, [api]);
  if (!status?.moved || !status.last) return null;
  const label = t("phone.backup.moved.row", { day: formatLongDay(status.last.at) });
  const remove = async () => {
    if (busy) return;
    setBusy(true);
    setFailed(false);
    try {
      await api.turnOff(true).catch((e: unknown) => {
        if (!PENDING.has(problemOf(e))) throw e;
      });
      setAsking(false);
      setStatus(await api.status().catch(() => null));
    } catch {
      setFailed(true);
    } finally {
      setBusy(false);
    }
  };
  return (
    <Group>
      <Row label={label} />
      <Row label={t("phone.backup.moved.delete")} danger onPress={() => setAsking(true)} />
      {asking ? (
        <BottomSheet label={label} onClose={() => setAsking(false)}>
          <div className="ph-sheet-body ph-stack" style={{ gap: 14 }}>
            <h2 className="ph-h1 ph-h1--small">{label}</h2>
            <p className="ph-body ph-ink2">
              {onPhone(
                ios,
                "phone.backup.off.deleteHint.ios",
                "phone.backup.off.deleteHint.android",
              )}
            </p>
            {failed ? (
              <p className="ph-error" role="alert">
                {t("ask.action.failed")}
              </p>
            ) : null}
            <Button kind="danger" disabled={busy} onClick={() => void remove()}>
              {t("phone.backup.moved.yes")}
            </Button>
            <button type="button" className="ph-textbtn" onClick={() => setAsking(false)}>
              {t("phone.backup.cancel")}
            </button>
          </div>
        </BottomSheet>
      ) : null}
    </Group>
  );
}
