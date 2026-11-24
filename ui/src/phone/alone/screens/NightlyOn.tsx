import { type ReactNode, useCallback, useEffect, useId, useRef, useState } from "react";
import { formatBytes } from "../../../copy/format";
import { t } from "../../../copy/t";
import type { BackupProgress } from "../../../engine/bindings";
import { strengthOf } from "../../../screens/backup/strength";
import { Switch, useHeading } from "../../native/screens/parts";
import { Button, Icon, Page } from "../../screens/ui";
import {
  type BackupApi,
  backupTime,
  type Connection,
  type LastNight,
  onPhone,
  problemOf,
} from "../nightly";
import { Chip, chips } from "./parts";

/** The phrase as the person is making it, held above the steps so Back keeps it. */
export interface PhraseDraft {
  /** Nocturne's phrase, shown once. */
  ours: string;
  /** "Use my own phrase instead". */
  own: boolean;
  typed: string;
  wifiOnly: boolean;
}

export const NEW_PHRASE: PhraseDraft = { ours: "", own: false, typed: "", wifiOnly: false };

export const phraseOf = (d: PhraseDraft): string => (d.own ? d.typed : d.ours);

/** Capitals and extra spaces never count; the words and their order do. */
export const samePhrase = (a: string, b: string): boolean => {
  const norm = (s: string) => s.normalize("NFC").toLowerCase().trim().split(/\s+/u).join(" ");
  return norm(a) !== "" && norm(a) === norm(b);
};

/** Easy to guess: below the lowest band of the strength estimate the backup file uses. */
export const easyToGuess = (phrase: string): boolean => {
  const level = strengthOf(phrase).level;
  return phrase.trim() !== "" && (level === "tooShort" || level === "weak");
};

export function BackOnly({ onBack, label }: { onBack(): void; label?: string }) {
  return (
    <header className="ph-bar ph-bar--back">
      <button
        type="button"
        className="ph-ib ph-ib--ink"
        onClick={onBack}
        aria-label={label ?? t("phone.backup.back")}
      >
        <Icon name="back" />
      </button>
    </header>
  );
}

function StepLabel({ step }: { step: number }) {
  return <p className="ph-mind-step">{t("phone.backup.step", { step })}</p>;
}

/** The labelled lines behind step 1's expander and under "About the backup". */
export function AboutLines({ ios, all = true }: { ios: boolean; all?: boolean }) {
  const thisPhone = <Chip>{t("phone.sheet.title")}</Chip>;
  const lines: [string, ReactNode][] = [
    [
      t("phone.backup.about.where.label"),
      onPhone(ios, "phone.backup.about.where.ios", "phone.backup.about.where.android"),
    ],
    [
      t("phone.backup.about.sees.label"),
      t("phone.backup.about.sees", {
        company: onPhone(
          ios,
          "phone.backup.about.company.ios",
          "phone.backup.about.company.android",
        ),
      }),
    ],
    [t("phone.backup.about.when.label"), t("phone.backup.about.when")],
    [t("phone.backup.about.many.label"), t("phone.backup.about.many")],
    [t("phone.backup.about.private.label"), t("phone.backup.about.private")],
    [t("phone.backup.about.own.label"), t("phone.backup.about.own")],
    [
      t("phone.backup.about.newPhone.label"),
      chips(ios ? "phone.backup.about.newPhone.ios" : "phone.backup.about.newPhone.android", {
        haveBackup: <Chip>{t("phone.alone.welcome.backup")}</Chip>,
      }),
    ],
    [
      t("phone.backup.about.hasMemory.label"),
      chips("phone.backup.about.hasMemory", {
        restore: <Chip>{t("phone.alone.thisPhone.restore")}</Chip>,
        thisPhone,
      }),
    ],
  ];
  if (all)
    lines.push(
      [
        t("phone.backup.about.file.label"),
        chips("phone.backup.about.file", {
          saveBackup: <Chip>{t("phone.alone.thisPhone.save")}</Chip>,
        }),
      ],
      [t("phone.backup.about.off.label"), chips("phone.backup.about.off")],
    );
  return (
    <dl className="ph-mind-lines">
      {lines.map(([label, line]) => (
        <div key={label}>
          <dt>{label}</dt>
          <dd>{line}</dd>
        </div>
      ))}
    </dl>
  );
}

type Blocked = "signedOut" | "turnedOff" | "noRoom" | "noAccount" | "refused" | "offline" | null;

/** Step 1: why, what goes and what is needed, and what stops it going on. */
export function BackupWhy({
  ios,
  size,
  connect,
  onBack,
  onNext,
  onNotNow,
}: {
  ios: boolean;
  /** The memory's size today, in bytes. */
  size: number | null;
  connect(): Promise<Connection>;
  onBack(): void;
  onNext(): void;
  onNotNow(): void;
}) {
  const [open, setOpen] = useState(false);
  const [blocked, setBlocked] = useState<Blocked>(null);
  const [free, setFree] = useState<number | null>(null);
  // On an iPhone, Continue waits until iCloud answers.
  const [ready, setReady] = useState(!ios);
  const [busy, setBusy] = useState(false);
  const heading = useHeading("why");
  const more = useId();
  const sized = formatBytes(size ?? 0);

  const read = useCallback(
    (c: Connection): boolean => {
      if (c.outcome === "connected") {
        if (c.free !== null && size !== null && c.free < size) {
          setFree(c.free);
          setBlocked("noRoom");
          return false;
        }
        setBlocked(null);
        return true;
      }
      if (c.outcome === "refused") setBlocked(ios ? "turnedOff" : "refused");
      else if (c.outcome === "noAccount") setBlocked(ios ? "signedOut" : "noAccount");
      else if (c.outcome === "turnedOff") setBlocked("turnedOff");
      else if (c.outcome === "offline") setBlocked("offline");
      else setBlocked(ios ? "turnedOff" : "refused");
      return false;
    },
    [ios, size],
  );

  // iCloud is looked at as the step opens and each time the phone comes back to Nocturne, so a
  // line goes by itself once the phone's state changes.
  useEffect(() => {
    let gone = false;
    const look = () => {
      if (ios)
        void connect().then(
          (c) => !gone && setReady(read(c)),
          () => !gone && setReady(false),
        );
      // On Android a line that needs the phone's settings clears when the person comes back.
      else setBlocked((b) => (b === "noAccount" || b === "offline" ? null : b));
    };
    if (ios) look();
    const front = () => {
      if (document.visibilityState === "visible") look();
    };
    const online = () => look();
    document.addEventListener("visibilitychange", front);
    window.addEventListener("online", online);
    return () => {
      gone = true;
      document.removeEventListener("visibilitychange", front);
      window.removeEventListener("online", online);
    };
  }, [ios, connect, read]);

  const go = async () => {
    if (busy) return;
    if (ios) return onNext();
    setBusy(true);
    try {
      if (read(await connect())) onNext();
    } catch {
      setBlocked("refused");
    } finally {
      setBusy(false);
    }
  };

  const line: Record<Exclude<Blocked, null>, string> = {
    signedOut: t("phone.backup.blocked.signedOut.ios"),
    turnedOff: t("phone.backup.blocked.turnedOff.ios"),
    noRoom: onPhone(ios, "phone.backup.blocked.noRoom.ios", "phone.backup.blocked.noRoom.android", {
      free: formatBytes(free ?? 0),
    }),
    noAccount: t("phone.backup.blocked.noAccount.android"),
    refused: t("phone.backup.blocked.refused.android"),
    offline: t("phone.backup.blocked.offline"),
  };
  // Refused and no room ask again with Continue; the rest wait for the phone's settings.
  const stuck =
    !ready || (blocked !== null && blocked !== "refused" && !(blocked === "noRoom" && !ios));

  return (
    <Page bar={<BackOnly onBack={onBack} />}>
      <main className="ph-main" style={{ gap: 14 }}>
        <StepLabel step={1} />
        <h1 className="ph-steph" tabIndex={-1} ref={heading}>
          {t("phone.backup.why.title")}
        </h1>
        <dl className="ph-mind-lines">
          <div>
            <dt>{t("phone.backup.why.goes.label")}</dt>
            <dd>{t("phone.backup.why.goes")}</dd>
          </div>
          <div>
            <dt>{t("phone.backup.why.who.label")}</dt>
            <dd>{onPhone(ios, "phone.backup.why.who.ios", "phone.backup.why.who.android")}</dd>
          </div>
          {size === null ? null : (
            <div>
              <dt>{t("phone.backup.why.need.label")}</dt>
              <dd>
                {onPhone(ios, "phone.backup.why.need.ios", "phone.backup.why.need.android", {
                  size: sized,
                })}
              </dd>
            </div>
          )}
        </dl>
        {blocked ? (
          <p className="ph-limit" role="status">
            {line[blocked]}
          </p>
        ) : null}
        <div className="ph-mind-more">
          <button
            type="button"
            className="ph-mind-expander"
            aria-expanded={open}
            aria-controls={more}
            onClick={() => setOpen((o) => !o)}
          >
            <span>{t("phone.backup.why.more")}</span>
            <span className={open ? "ph-mind-chevron ph-mind-chevron--open" : "ph-mind-chevron"}>
              <Icon name="forward" size={18} />
            </span>
          </button>
          {open ? (
            <div id={more} className="ph-mind-what">
              <AboutLines ios={ios} />
            </div>
          ) : null}
        </div>
      </main>
      <footer className="ph-foot">
        <Button kind="primary" disabled={stuck || busy} onClick={() => void go()}>
          {t("phone.backup.continue")}
        </Button>
        <button type="button" className="ph-textbtn" onClick={onNotNow}>
          {t("phone.backup.notNow")}
        </button>
        <p className="ph-mind-barline">{t("phone.backup.why.barLine")}</p>
      </footer>
    </Page>
  );
}

/** Six words in large type, read aloud one by one with their place. */
export function PhraseWords({ phrase }: { phrase: string }) {
  const words = phrase.split(/\s+/u).filter(Boolean);
  return (
    <ol className="ph-nb-phrase" aria-label={t("phone.backup.phrase.label")}>
      {words.map((word, i) => (
        <li key={`${i}-${word}`} aria-label={t("phone.backup.phrase.word", { n: i + 1, word })}>
          {word}
        </li>
      ))}
    </ol>
  );
}

/** A minute after "Copy", the phrase leaves where it was pasted from. */
const COPY_CLEARS_MS = 60_000;

function copyPhrase(phrase: string) {
  const clip = navigator.clipboard;
  if (!clip) return;
  void clip.writeText(phrase).then(
    () => setTimeout(() => void clip.writeText("").catch(() => undefined), COPY_CLEARS_MS),
    () => undefined,
  );
}

function SafeNote() {
  return (
    <div className="ph-nb-note">
      <strong>{t("phone.backup.phrase.note.title")}</strong>
      <p>{t("phone.backup.phrase.note")}</p>
    </div>
  );
}

/** Step 2: Nocturne's phrase, or the person's own. */
export function PhraseStep({
  draft,
  onDraft,
  newPhrase,
  onBack,
  onNext,
}: {
  draft: PhraseDraft;
  onDraft(next: PhraseDraft): void;
  newPhrase(): Promise<string>;
  onBack(): void;
  onNext(): void;
}) {
  const [d, setD] = useState(draft);
  const heading = useHeading("phrase");
  const id = useId();
  const hint = useId();
  const set = (next: PhraseDraft) => {
    setD(next);
    onDraft(next);
  };
  const another = useCallback(
    () =>
      void newPhrase().then(
        (ours) =>
          setD((now) => {
            const next = { ...now, ours };
            onDraft(next);
            return next;
          }),
        () => undefined,
      ),
    [newPhrase, onDraft],
  );
  // The first phrase is made as the step first opens.
  const asked = useRef(false);
  useEffect(() => {
    if (asked.current || d.ours !== "") return;
    asked.current = true;
    another();
  }, [d.ours, another]);

  const ready = d.own ? d.typed.trim() !== "" : d.ours !== "";
  const weak = d.own && easyToGuess(d.typed);
  return (
    <Page bar={<BackOnly onBack={onBack} />}>
      <main className="ph-main" style={{ gap: 14 }}>
        <StepLabel step={2} />
        <h1 className="ph-steph" tabIndex={-1} ref={heading}>
          {t("phone.backup.phrase.title")}
        </h1>
        {d.own ? (
          <>
            <p className="ph-body ph-ink2">{t("phone.backup.phrase.leadOwn")}</p>
            <div className="ph-field">
              <label htmlFor={id}>{t("phone.backup.phrase.label")}</label>
              <input
                id={id}
                className="ph-input"
                type="text"
                autoComplete="new-password"
                autoCapitalize="none"
                spellCheck={false}
                aria-describedby={hint}
                value={d.typed}
                onChange={(e) => set({ ...d, typed: e.target.value })}
              />
              <span id={hint} className="ph-nb-hint">
                {t("phone.backup.phrase.ownHint")}
              </span>
            </div>
            {weak ? (
              <div className="ph-limit ph-nb-say" role="status">
                <span>{t("phone.backup.phrase.weak")}</span>
                <button type="button" className="ph-link" onClick={() => set({ ...d, own: false })}>
                  {t("phone.backup.phrase.weakUse")}
                </button>
              </div>
            ) : null}
          </>
        ) : (
          <>
            <p className="ph-body ph-ink2">{t("phone.backup.phrase.lead")}</p>
            <div className="ph-nb-card">
              <PhraseWords phrase={d.ours} />
              <div className="ph-nb-links">
                <button type="button" className="ph-link" onClick={another}>
                  {t("phone.backup.phrase.another")}
                </button>
                <button type="button" className="ph-link" onClick={() => copyPhrase(d.ours)}>
                  {t("phone.backup.phrase.copy")}
                </button>
              </div>
            </div>
          </>
        )}
        {d.own ? null : (
          <button type="button" className="ph-link" onClick={() => set({ ...d, own: true })}>
            {t("phone.backup.phrase.useOwn")}
          </button>
        )}
        <SafeNote />
        {d.own ? (
          <button type="button" className="ph-link" onClick={() => set({ ...d, own: false })}>
            {t("phone.backup.phrase.useOurs")}
          </button>
        ) : null}
      </main>
      <footer className="ph-foot">
        <Button kind="primary" disabled={!ready} onClick={onNext}>
          {t("phone.backup.next")}
        </Button>
        <button type="button" className="ph-textbtn" onClick={onBack}>
          {t("phone.backup.back")}
        </button>
        {!ready && d.own ? (
          <p className="ph-mind-barline">{t("phone.backup.phrase.typeToGo")}</p>
        ) : null}
      </footer>
    </Page>
  );
}

/** Step 3: the phrase typed back, and Wi-Fi only. `save` for changing the phrase. */
export function CheckStep({
  draft,
  onDraft,
  size,
  save = false,
  onBack,
  onDone,
}: {
  draft: PhraseDraft;
  onDraft(next: PhraseDraft): void;
  size: number | null;
  /** "Save" in place of "Turn on", with no Wi-Fi only row. */
  save?: boolean;
  onBack(): void;
  /** Rejects when it could not be done; the step stays. */
  onDone(phrase: string, wifiOnly: boolean): Promise<void>;
}) {
  const [typed, setTyped] = useState("");
  const [wrong, setWrong] = useState(false);
  const [wifiOnly, setWifiOnly] = useState(draft.wifiOnly);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const heading = useHeading("check");
  const id = useId();
  const err = useId();
  const phrase = phraseOf(draft);
  const ready = typed.trim() !== "";

  const go = async () => {
    if (!ready || busy) return;
    if (!samePhrase(typed, phrase)) return setWrong(true);
    setBusy(true);
    setFailed(false);
    try {
      await onDone(phrase, wifiOnly);
    } catch {
      setFailed(true);
      setBusy(false);
    }
  };

  return (
    <Page bar={<BackOnly onBack={onBack} />}>
      <main className="ph-main" style={{ gap: 14 }}>
        <StepLabel step={3} />
        <h1 className="ph-steph" tabIndex={-1} ref={heading}>
          {t("phone.backup.check.title")}
        </h1>
        <p className="ph-body ph-ink2">{t("phone.backup.check.lead")}</p>
        <div className="ph-field">
          <label htmlFor={id}>{t("phone.backup.phrase.label")}</label>
          <input
            id={id}
            className={wrong ? "ph-input ph-nb-input--wrong" : "ph-input"}
            type="text"
            autoComplete="off"
            autoCapitalize="none"
            spellCheck={false}
            aria-invalid={wrong || undefined}
            aria-describedby={wrong ? err : undefined}
            value={typed}
            onChange={(e) => {
              setTyped(e.target.value);
              setWrong(false);
            }}
          />
          {wrong ? (
            <span id={err} className="ph-error" role="alert">
              {t("phone.backup.check.wrong")}
            </span>
          ) : null}
        </div>
        {save ? null : (
          <div className="ph-nb-group">
            <Switch
              label={t("phone.backup.wifi")}
              hint={
                wifiOnly
                  ? t("phone.backup.wifi.on")
                  : t("phone.backup.wifi.off", { size: formatBytes(size ?? 0) })
              }
              on={wifiOnly}
              onChange={(on) => {
                setWifiOnly(on);
                onDraft({ ...draft, wifiOnly: on });
              }}
            />
          </div>
        )}
        {failed ? (
          <p className="ph-error" role="alert">
            {t("ask.action.failed")}
          </p>
        ) : null}
      </main>
      <footer className="ph-foot">
        <Button kind="primary" disabled={!ready || busy} onClick={() => void go()}>
          {save ? t("phone.backup.check.save") : t("phone.backup.check.turnOn")}
        </Button>
        <button type="button" className="ph-textbtn" onClick={onBack}>
          {t("phone.backup.back")}
        </button>
        {ready ? null : <p className="ph-mind-barline">{t("phone.backup.check.typeToGo")}</p>}
      </footer>
    </Page>
  );
}

/** The recovery key in its six groups, read aloud group by group. */
export function KeyGroups({ text }: { text: string }) {
  const groups = text.split(/[\s-]+/u).filter(Boolean);
  return (
    <ol className="ph-nb-key" aria-label={t("phone.backup.key.label")}>
      {groups.map((group, i) => (
        <li key={`${i}-${group}`} aria-label={t("phone.backup.key.group", { n: i + 1, group })}>
          {group}
        </li>
      ))}
    </ol>
  );
}

/**
 * A recovery key, shown this once: offered after "Turn on" ("Not now"), or made later from This
 * phone ("Cancel"). After "Print or save…" the button reads "Done".
 */
export function KeyShown({
  text,
  later = false,
  save,
  keep,
  onSaved,
  onClose,
}: {
  text: string;
  later?: boolean;
  save(text: string): Promise<"saved" | "cancelled" | "failed">;
  /** The key went to the phone's sheet: from now on it opens the backup. */
  keep(): Promise<void>;
  /** The key was saved and opens the backup. */
  onSaved(): void;
  /** `saved`: closed with Done, after the key was saved. Before that, the key opens nothing. */
  onClose(saved: boolean): void;
}) {
  const [saved, setSaved] = useState(false);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const heading = useHeading("key");
  const printOrSave = async () => {
    if (busy) return;
    setBusy(true);
    setFailed(false);
    try {
      if ((await save(text)) !== "saved") return;
      await keep();
      setSaved(true);
      onSaved();
    } catch {
      setFailed(true);
    } finally {
      setBusy(false);
    }
  };
  return (
    <Page bar={null}>
      <main className="ph-main" style={{ gap: 14 }}>
        <div className="ph-hero" style={{ paddingTop: 40 }}>
          <span className="ph-hero-icon">
            <Icon name="shield" size={32} />
          </span>
          <h1 className="ph-steph" tabIndex={-1} ref={heading}>
            {later ? t("phone.backup.newKey.title") : t("phone.backup.key.title")}
          </h1>
          <p className="ph-body ph-ink2">
            {later ? t("phone.backup.newKey.body") : t("phone.backup.key.body")}
          </p>
        </div>
        <KeyGroups text={text} />
        {failed ? (
          <p className="ph-error" role="alert">
            {t("ask.action.failed")}
          </p>
        ) : null}
      </main>
      <footer className="ph-foot">
        {saved ? (
          <Button kind="primary" onClick={() => onClose(true)}>
            {t("phone.backup.key.done")}
          </Button>
        ) : (
          <>
            <Button kind="primary" disabled={busy} onClick={() => void printOrSave()}>
              {t("phone.backup.key.print")}
            </Button>
            <button type="button" className="ph-textbtn" onClick={() => onClose(false)}>
              {later ? t("phone.backup.key.cancel") : t("phone.backup.notNow")}
            </button>
          </>
        )}
        <p className="ph-mind-barline">{t("phone.backup.key.foot")}</p>
      </footer>
    </Page>
  );
}

/** The first backup, then "Your memory is backed up." */
export function FirstBackup({
  api,
  ios,
  onDone,
}: {
  api: Pick<BackupApi, "now">;
  ios: boolean;
  /** "Use Nocturne" while it runs, or OK once it is done: the backup carries on either way. */
  onDone(): void;
}) {
  const [progress, setProgress] = useState<BackupProgress | null>(null);
  const [done, setDone] = useState<LastNight | null>(null);
  const [wifi, setWifi] = useState(false);
  const [waiting, setWaiting] = useState(false);
  const heading = useHeading(done ? "done" : "first");
  const started = useRef(false);

  useEffect(() => {
    if (started.current) return;
    started.current = true;
    void api.now(false, setProgress).then(setDone, (e: unknown) => {
      if (problemOf(e) === "waitingForWifi") setWifi(true);
      // It carries on tonight, as the note says.
      setProgress(null);
      setWaiting(true);
    });
  }, [api]);

  if (done)
    return (
      <Page bar={null}>
        <main className="ph-main">
          <div className="ph-hero" style={{ paddingTop: 96 }} role="status">
            <span className="ph-hero-icon">
              <Icon name="check" size={32} />
            </span>
            <h1 className="ph-h1" tabIndex={-1} ref={heading}>
              {t("phone.backup.done.title")}
            </h1>
            <p className="ph-body ph-ink2">
              {onPhone(ios, "phone.backup.done.where.ios", "phone.backup.done.where.android", {
                time: backupTime(done.at),
              })}
            </p>
            <p className="ph-body ph-ink2">{t("phone.backup.done.after")}</p>
          </div>
          <div className="ph-spacer" />
          <Button kind="primary" onClick={onDone}>
            {t("phone.backup.ok")}
          </Button>
        </main>
      </Page>
    );

  const share = progress && progress.total > 0 ? progress.done / progress.total : 0;
  const detail = progress
    ? t("phone.backup.progress", {
        done: formatBytes(progress.done),
        size: formatBytes(progress.total),
      })
    : null;
  return (
    <Page bar={null}>
      <main className="ph-main" aria-busy={waiting ? undefined : true}>
        <div className="ph-hero" style={{ paddingTop: 96 }}>
          <span className="ph-hero-icon">
            <Icon name="refresh" size={32} />
          </span>
          <h1 className="ph-steph" tabIndex={-1} ref={heading}>
            {t("phone.backup.first.title")}
          </h1>
        </div>
        <div
          className="ph-progress"
          role="progressbar"
          aria-label={t("phone.backup.first.title")}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={Math.round(share * 100)}
          aria-valuetext={detail ?? undefined}
        >
          <span style={{ width: `${Math.round(share * 100)}%` }} />
        </div>
        {detail ? <p className="ph-small ph-center">{detail}</p> : null}
        <p className="ph-body ph-ink2 ph-center" role={wifi ? "status" : undefined}>
          {wifi ? t("phone.backup.first.wifi") : t("phone.backup.first.note")}
        </p>
        <div className="ph-spacer" />
        <Button onClick={onDone}>{t("phone.backup.first.use")}</Button>
      </main>
    </Page>
  );
}

/** After "Turn on": the recovery key, offered once, then the first backup. */
export function AfterTurnOn({
  api,
  ios,
  onKeyMade,
  onDone,
}: {
  api: Pick<BackupApi, "recoveryKey" | "keepRecoveryKey" | "dropRecoveryKey" | "saveKey" | "now">;
  ios: boolean;
  onKeyMade(): void;
  onDone(): void;
}) {
  const [key, setKey] = useState<string | null>(null);
  const [first, setFirst] = useState(false);
  const asked = useRef(false);
  useEffect(() => {
    if (asked.current) return;
    asked.current = true;
    void api.recoveryKey().then(setKey, () => setFirst(true));
  }, [api]);
  if (first) return <FirstBackup api={api} ios={ios} onDone={onDone} />;
  if (!key) return null;
  return (
    <KeyShown
      text={key}
      save={api.saveKey}
      keep={api.keepRecoveryKey}
      onSaved={onKeyMade}
      onClose={(saved) => {
        // A key that was never saved opens nothing.
        if (!saved) void api.dropRecoveryKey().catch(() => undefined);
        setKey(null);
        setFirst(true);
      }}
    />
  );
}
