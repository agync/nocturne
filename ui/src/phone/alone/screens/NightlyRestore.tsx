import { type ReactNode, useId, useRef, useState } from "react";
import { formatBytes } from "../../../copy/format";
import { t } from "../../../copy/t";
import type { BackupProgress } from "../../../engine/bindings";
import { progressDetail, useProgressEta } from "../../../screens/backup/progress";
import { useHeading } from "../../native/screens/parts";
import { BottomSheet } from "../../screens/Dialog";
import { Button, Icon, type IconName, Page } from "../../screens/ui";
import {
  type BackupApi,
  backupDay,
  backupTime,
  type FoundBackup,
  type NightView,
  type Opening,
  onPhone,
  problemOf,
  type Restored,
} from "../nightly";
import { BackOnly, CheckStep, NEW_PHRASE, type PhraseDraft, PhraseStep } from "./NightlyOn";
import { Chip, chips } from "./parts";

type Step =
  | { name: "where"; refused: boolean }
  | { name: "looking" }
  | { name: "none"; why: "none" | "icloudOff" | "offline" }
  | { name: "found" }
  | { name: "nights" }
  | { name: "merge" }
  | { name: "getting" }
  | { name: "problem"; why: "room" | "missing" | "newer" | "lost" | "stopped" }
  | { name: "restored"; restored: Extract<Restored, { kind: "replaced" }> }
  | { name: "merged"; restored: Extract<Restored, { kind: "broughtIn" }> }
  | { name: "newPhrase" }
  | { name: "phrase" }
  | { name: "check" }
  | { name: "inUse" };

/** Wrong this many times, the line points to the password manager and the recovery key. */
const WRONG_AGAIN = 3;

function Hero({
  icon,
  title,
  children,
  ember = false,
}: {
  icon?: IconName;
  title: string;
  children?: ReactNode;
  ember?: boolean;
}) {
  const heading = useHeading(title);
  return (
    <div className="ph-hero" style={{ paddingTop: 96 }}>
      {icon ? (
        <span className={ember ? "ph-hero-icon ph-nb-ember" : "ph-hero-icon"}>
          <Icon name={icon} size={32} />
        </span>
      ) : null}
      <h1 className="ph-steph" tabIndex={-1} ref={heading}>
        {title}
      </h1>
      {children}
    </div>
  );
}

/** One screen of the restore with its buttons at the foot. */
function Screen({
  bar = null,
  children,
  foot,
}: {
  bar?: ReactNode;
  children: ReactNode;
  foot?: ReactNode;
}) {
  return (
    <Page bar={bar}>
      <main className="ph-main" style={{ gap: 14 }}>
        {children}
      </main>
      {foot ? <footer className="ph-foot">{foot}</footer> : null}
    </Page>
  );
}

/** A restore running: its stage, the bar, and Stop while it can be stopped. */
function Bringing({
  title,
  label,
  progress,
  body,
  onStop,
}: {
  title: string;
  label: string;
  progress: BackupProgress | null;
  body: string;
  onStop?(): void;
}) {
  const heading = useHeading(title);
  const eta = useProgressEta(progress);
  const share = progress && progress.total > 0 ? progress.done / progress.total : 0;
  const detail = progress && progress.total > 0 ? progressDetail(progress, eta) : null;
  return (
    <Page bar={null}>
      <main className="ph-main" aria-busy="true" style={{ gap: 14 }}>
        <div className="ph-hero" style={{ paddingTop: 96 }}>
          <h1 className="ph-steph" tabIndex={-1} ref={heading}>
            {title}
          </h1>
        </div>
        <p className="ph-lbl" role="status">
          {label}
        </p>
        <div
          className="ph-progress"
          role="progressbar"
          aria-label={label}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={Math.round(share * 100)}
          aria-valuetext={detail ?? undefined}
        >
          <span style={{ width: `${Math.round(share * 100)}%` }} />
        </div>
        {detail ? <p className="ph-small">{detail}</p> : null}
        <p className="ph-body ph-ink2">{body}</p>
        <div className="ph-spacer" />
        {onStop ? (
          <Button disabled={progress === null} onClick={onStop}>
            {t("backup.stop")}
          </Button>
        ) : null}
      </main>
    </Page>
  );
}

/**
 * "Where is your backup?", then the phrase or the recovery key, the night, and bringing it back:
 * as it was onto an empty phone, or its things into the memory here. Then whether this phone
 * makes the backups.
 */
export function NightlyRestore({
  api,
  ios,
  holds,
  things,
  freeSpace,
  onFile,
  onBack,
  onDone,
  now = Date.now,
}: {
  /** The clock, so "today" is this phone's today. */
  now?(): number;
  api: BackupApi;
  ios: boolean;
  /** This phone holds a memory, so the backup's things join it. */
  holds: boolean;
  /** How many things the memory here holds. */
  things?(): Promise<number>;
  freeSpace(): Promise<number | null>;
  /** "A backup file": the file restore. */
  onFile(): void;
  onBack(): void;
  onDone(): void;
}) {
  const [step, setStep] = useState<Step>({ name: "where", refused: false });
  const [online, setOnline] = useState(true);
  const [found, setFound] = useState<FoundBackup | null>(null);
  const [withKey, setWithKey] = useState(false);
  const [text, setText] = useState("");
  const [wrong, setWrong] = useState(0);
  const [nights, setNights] = useState<NightView[] | null>(null);
  const [night, setNight] = useState<number | null>(null);
  const [picked, setPicked] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<BackupProgress | null>(null);
  const [asking, setAsking] = useState(false);
  const [here, setHere] = useState<number | null>(null);
  const [free, setFree] = useState<number | null>(null);
  const [draft, setDraft] = useState<PhraseDraft>(NEW_PHRASE);
  // The night's things are being added: Stop is no longer offered.
  const adding = useRef(false);
  const stopped = useRef(false);
  const name = useId();
  const field = useId();
  const err = useId();

  const opening = (): Opening =>
    withKey ? { with: "recoveryKey", text } : { with: "phrase", text };
  const chosen = nights?.find((n) => n.count === night) ?? nights?.[0] ?? null;
  const size = chosen?.bytes ?? found?.bytes ?? 0;

  const look = async () => {
    setStep({ name: "looking" });
    try {
      const f = await api.found();
      if (!f) return setStep({ name: "none", why: "none" });
      setFound(f);
      setStep({ name: "found" });
    } catch (e) {
      const p = problemOf(e);
      if (p === "offline") return setStep({ name: "none", why: "offline" });
      if (ios && (p === "noAccess" || p === "noPermission"))
        return setStep({ name: "none", why: "icloudOff" });
      setStep({ name: "none", why: "none" });
    }
  };

  const connect = async (choose: boolean) => {
    if (busy) return;
    setBusy(true);
    try {
      const c = await api.connect(choose);
      if (c.outcome === "connected") return void (await look());
      if (c.outcome === "offline") return setStep({ name: "none", why: "offline" });
      if (ios)
        return setStep({ name: "none", why: c.outcome === "noAccount" ? "none" : "icloudOff" });
      setStep({ name: "where", refused: true });
    } catch {
      setStep({ name: "where", refused: !ios });
    } finally {
      setBusy(false);
    }
  };

  /** The phrase is checked by opening the list of nights. */
  const open = async (): Promise<NightView[] | null> => {
    try {
      const list = await api.nights(opening());
      setNights(list);
      setWrong(0);
      return list;
    } catch (e) {
      const p = problemOf(e);
      if (p === "wrongPhrase" || p === "noPhrase") setWrong((n) => n + 1);
      else if (p === "offline") setStep({ name: "none", why: "offline" });
      else setStep({ name: "problem", why: "lost" });
      return null;
    }
  };

  const bringBack = async () => {
    if (busy || text.trim() === "") return;
    setBusy(true);
    try {
      if (!(await open())) return;
      if (!holds) return void (await restore());
      const [count, room] = await Promise.all([
        things ? things().catch(() => null) : Promise.resolve(null),
        freeSpace().catch(() => null),
      ]);
      setHere(count);
      setFree(room);
      setStep({ name: "merge" });
    } finally {
      setBusy(false);
    }
  };

  const restore = async () => {
    stopped.current = false;
    adding.current = false;
    setProgress(null);
    setStep({ name: "getting" });
    try {
      const done = await api.restore(opening(), night, (p) => {
        setProgress(p);
        if (p.total > 0 && p.done >= p.total) adding.current = true;
      });
      if (done.kind === "replaced") setStep({ name: "restored", restored: done });
      else setStep({ name: "merged", restored: done });
    } catch (e) {
      const p = stopped.current ? "stopped" : problemOf(e);
      if (p === "stopped") return setStep({ name: "problem", why: "stopped" });
      if (p === "noRoom") {
        setFree(await freeSpace().catch(() => null));
        return setStep({ name: "problem", why: "room" });
      }
      if (p === "damaged" || p === "noBackup") return setStep({ name: "problem", why: "missing" });
      if (p === "newer") return setStep({ name: "problem", why: "newer" });
      if (p === "wrongPhrase") {
        setWrong((n) => n + 1);
        return setStep({ name: "found" });
      }
      if (p === "offline") return setStep({ name: "none", why: "offline" });
      setStep({ name: "problem", why: "lost" });
    } finally {
      setProgress(null);
    }
  };

  const stop = () => {
    stopped.current = true;
    setAsking(false);
    void api.stop();
  };

  /** After a restore: a new phrase when the recovery key opened it, then who makes backups. */
  const after = async () => {
    const s = await api.status().catch(() => null);
    // This phone's own backup: it goes on making them.
    if (s?.on) return onDone();
    if (withKey) return setStep({ name: "newPhrase" });
    setStep({ name: "inUse" });
  };

  switch (step.name) {
    case "where":
      return (
        <Screen
          bar={<BackOnly onBack={onBack} />}
          foot={
            <>
              <Button
                kind="primary"
                disabled={busy}
                onClick={() => (online ? void connect(false) : onFile())}
              >
                {step.refused && online ? t("phone.backup.tryAgain") : t("phone.backup.continue")}
              </Button>
              <button type="button" className="ph-textbtn" onClick={onBack}>
                {t("phone.backup.back")}
              </button>
            </>
          }
        >
          <WhereHeading />
          <fieldset className="ph-options ph-mind-ways">
            <legend className="sr-only">{t("phone.backup.where.legend")}</legend>
            {(
              [
                [
                  true,
                  onPhone(
                    ios,
                    "phone.backup.where.online.ios",
                    "phone.backup.where.online.android",
                  ),
                  t("phone.backup.where.online.hint"),
                ],
                [false, t("phone.backup.where.file"), t("phone.backup.where.file.hint")],
              ] as const
            ).map(([isOnline, title, hint]) => (
              <label key={title} className="ph-option ph-mind-way">
                <input
                  type="radio"
                  name={name}
                  checked={online === isOnline}
                  onChange={() => setOnline(isOnline)}
                />
                <span className="ph-radio" aria-hidden="true" />
                <span>
                  <strong>{title}</strong>
                  <span className="ph-mind-way-hint">{hint}</span>
                </span>
              </label>
            ))}
          </fieldset>
          {step.refused && online ? (
            <p className="ph-limit" role="status">
              {t("phone.backup.where.refused")}
            </p>
          ) : null}
        </Screen>
      );
    case "looking":
      return (
        <Screen>
          <Hero title={t("phone.backup.looking")}>
            <span className="ph-dot ph-dot--breathe" aria-hidden="true" />
          </Hero>
        </Screen>
      );
    case "none": {
      if (step.why === "offline")
        return (
          <Screen
            foot={
              <>
                <Button kind="primary" onClick={() => void connect(false)}>
                  {t("phone.backup.tryAgain")}
                </Button>
                <button type="button" className="ph-textbtn" onClick={onBack}>
                  {t("phone.backup.back")}
                </button>
              </>
            }
          >
            <Hero icon="info" title={t("phone.backup.none.offline")} />
          </Screen>
        );
      if (step.why === "icloudOff")
        return (
          <Screen
            foot={
              <Button kind="primary" onClick={onBack}>
                {t("phone.backup.back")}
              </Button>
            }
          >
            <Hero icon="info" title={t("phone.backup.none.icloudOff")} />
          </Screen>
        );
      return ios ? (
        <Screen
          foot={
            <Button kind="primary" onClick={onBack}>
              {t("phone.backup.back")}
            </Button>
          }
        >
          <Hero icon="info" title={t("phone.backup.none.ios")}>
            <p className="ph-body ph-ink2">{t("phone.backup.none.ios.body")}</p>
          </Hero>
        </Screen>
      ) : (
        <Screen
          foot={
            <>
              <Button kind="primary" disabled={busy} onClick={() => void connect(true)}>
                {t("phone.backup.none.another")}
              </Button>
              <button type="button" className="ph-textbtn" onClick={onBack}>
                {t("phone.backup.back")}
              </button>
            </>
          }
        >
          <Hero icon="info" title={t("phone.backup.none.android")} />
        </Screen>
      );
    }
    case "found": {
      const shown = chosen ? { at: chosen.madeAt, bytes: chosen.bytes } : found;
      const error =
        wrong === 0
          ? null
          : withKey
            ? t("phone.backup.found.wrongKey")
            : wrong >= WRONG_AGAIN
              ? t("phone.backup.found.wrongAgain")
              : t("phone.backup.found.wrong");
      return (
        <Screen
          bar={<BackOnly onBack={() => setStep({ name: "where", refused: false })} />}
          foot={
            <>
              <Button
                kind="primary"
                disabled={busy || text.trim() === ""}
                onClick={() => void bringBack()}
              >
                {t("phone.backup.found.bring")}
              </Button>
              <button
                type="button"
                className="ph-textbtn"
                onClick={() => setStep({ name: "where", refused: false })}
              >
                {t("phone.backup.back")}
              </button>
            </>
          }
        >
          <FoundHeading />
          <p className="ph-body ph-ink2">
            {withKey ? t("phone.backup.found.leadKey") : t("phone.backup.found.lead")}
          </p>
          {shown?.at ? (
            <div className="ph-nb-found">
              <Icon name="lock" size={22} />
              <span className="ph-srow-text">
                <span className="ph-srow-label">
                  {t("phone.backup.found.card", {
                    day: backupDay(shown.at, new Date(now())),
                    time: backupTime(shown.at),
                    size: formatBytes(shown.bytes),
                  })}
                </span>
                <button
                  type="button"
                  className="ph-link"
                  aria-disabled={text.trim() === "" || busy ? true : undefined}
                  onClick={async () => {
                    if (text.trim() === "" || busy) return;
                    setBusy(true);
                    const list = nights ?? (await open());
                    setBusy(false);
                    if (!list) return;
                    setPicked(night ?? list[0]?.count ?? null);
                    setStep({ name: "nights" });
                  }}
                >
                  {t("phone.backup.found.earlier")}
                </button>
              </span>
            </div>
          ) : null}
          <div className="ph-field">
            <label htmlFor={field}>
              {withKey ? t("phone.backup.key.label") : t("phone.backup.phrase.label")}
            </label>
            <input
              id={field}
              className={error ? "ph-input ph-nb-input--wrong" : "ph-input"}
              type="text"
              autoComplete="current-password"
              autoCapitalize={withKey ? "characters" : "none"}
              spellCheck={false}
              aria-invalid={error ? true : undefined}
              aria-describedby={error ? err : undefined}
              value={text}
              onChange={(e) => {
                setText(e.target.value);
                setNights(null);
                setNight(null);
              }}
            />
            {error ? (
              <span id={err} className="ph-error" role="alert">
                {error}
              </span>
            ) : null}
          </div>
          <button
            type="button"
            className="ph-link"
            onClick={() => {
              setWithKey((k) => !k);
              setText("");
              setWrong(0);
              setNights(null);
              setNight(null);
            }}
          >
            {withKey ? t("phone.backup.found.usePhrase") : t("phone.backup.found.useKey")}
          </button>
        </Screen>
      );
    }
    case "nights":
      return (
        <Page
          bar={
            <header className="ph-bar ph-bar--back">
              <button
                type="button"
                className="ph-ib ph-ib--ink"
                onClick={() => setStep({ name: "found" })}
                aria-label={t("phone.cancel")}
              >
                <Icon name="close" />
              </button>
              <h1 className="ph-bar-title">{t("phone.backup.nights.title")}</h1>
              <button
                type="button"
                className="ph-textbtn ph-nb-done"
                onClick={() => {
                  setNight(picked);
                  setStep({ name: "found" });
                }}
              >
                {t("phone.backup.nights.done")}
              </button>
            </header>
          }
        >
          <main className="ph-main" style={{ gap: 14 }}>
            <p className="ph-body ph-ink2">{t("phone.backup.nights.lead")}</p>
            <fieldset className="ph-nb-nights">
              <legend className="sr-only">{t("phone.backup.nights.title")}</legend>
              {(nights ?? []).map((n, i) => (
                <label key={n.count} className="ph-nb-night">
                  <input
                    type="radio"
                    name={name}
                    checked={(picked ?? nights?.[0]?.count) === n.count}
                    onChange={() => setPicked(n.count)}
                  />
                  <span className="ph-radio" aria-hidden="true" />
                  <span>
                    {t("phone.backup.nights.night", {
                      day: backupDay(n.madeAt, new Date(now()), true),
                      time: backupTime(n.madeAt),
                    })}
                  </span>
                  {i === 0 ? (
                    <span className="ph-nb-newest">{t("phone.backup.nights.newest")}</span>
                  ) : null}
                  <span className="ph-nb-size">{formatBytes(n.bytes)}</span>
                </label>
              ))}
            </fieldset>
          </main>
        </Page>
      );
    case "merge": {
      const short = free !== null && free < size;
      return (
        <Screen
          bar={<BackOnly onBack={onBack} label={t("phone.sheet.title")} />}
          foot={
            <>
              <Button kind="primary" onClick={() => void restore()}>
                {t("phone.backup.merge.yes")}
              </Button>
              <button type="button" className="ph-textbtn" onClick={onBack}>
                {t("phone.backup.notNow")}
              </button>
            </>
          }
        >
          <MergeHeading />
          <div className="ph-nb-both">
            <div>
              <b>{t("phone.backup.merge.inBackup")}</b>
              {chosen ? (
                <span>
                  {t("phone.backup.nights.night", {
                    day: backupDay(chosen.madeAt, new Date(now()), true),
                    time: backupTime(chosen.madeAt),
                  })}
                </span>
              ) : null}
              <span className="ph-small">
                {t("phone.backup.merge.about", { size: formatBytes(size) })}
              </span>
            </div>
            <div>
              <b>{t("phone.backup.merge.onPhone")}</b>
              {here !== null ? (
                <span>{t("phone.backup.merge.things", { count: here })}</span>
              ) : null}
            </div>
          </div>
          <p className="ph-body ph-ink2">{t("phone.backup.merge.lead")}</p>
          <p className="ph-body">{t("phone.backup.merge.what")}</p>
          {short ? (
            <p className="ph-limit" role="status">
              {chips("phone.backup.merge.room", {
                size: formatBytes(size),
                free: formatBytes(free ?? 0),
                bringThemIn: <Chip>{t("phone.backup.merge.yes")}</Chip>,
              })}
            </p>
          ) : null}
        </Screen>
      );
    }
    case "getting":
      if (holds) {
        const isAdding = progress !== null && progress.total > 0 && progress.done >= progress.total;
        return (
          <>
            <Bringing
              title={t("phone.backup.adding.title")}
              label={isAdding ? t("phone.backup.adding.adding") : t("phone.backup.adding.ready")}
              progress={progress}
              body={t("phone.backup.adding.body")}
              onStop={isAdding ? undefined : () => setAsking(true)}
            />
            {asking && !isAdding ? (
              <BottomSheet
                label={t("phone.backup.adding.stopAsk")}
                onClose={() => setAsking(false)}
              >
                <div className="ph-sheet-body ph-stack" style={{ gap: 14 }}>
                  <h2 className="ph-h1 ph-h1--small">{t("phone.backup.adding.stopAsk")}</h2>
                  <Button kind="danger" onClick={stop}>
                    {t("phone.backup.adding.stop")}
                  </Button>
                  <button type="button" className="ph-textbtn" onClick={() => setAsking(false)}>
                    {t("phone.backup.adding.keepGoing")}
                  </button>
                </div>
              </BottomSheet>
            ) : null}
          </>
        );
      }
      return (
        <Bringing
          title={t("phone.backup.getting.title")}
          label={t("phone.backup.getting.stage")}
          progress={progress}
          body={t("phone.backup.getting.body")}
          onStop={stop}
        />
      );
    case "problem": {
      const again = (
        <Button kind="primary" onClick={() => setStep({ name: "found" })}>
          {t("phone.backup.tryAgain")}
        </Button>
      );
      if (step.why === "stopped")
        return (
          <Screen
            foot={
              <Button kind="primary" onClick={onBack}>
                {t("phone.backup.back")}
              </Button>
            }
          >
            {holds ? (
              <Hero icon="info" title={t("phone.backup.adding.stopped")} />
            ) : (
              <Hero icon="info" title={t("phone.backup.getting.stopped")}>
                <p className="ph-body ph-ink2">{t("phone.backup.getting.stoppedBody")}</p>
              </Hero>
            )}
          </Screen>
        );
      if (step.why === "room")
        return (
          <Screen foot={again}>
            <Hero icon="info" ember title={t("phone.backup.problem.room.title")}>
              <p className="ph-body ph-ink2">
                {t("phone.backup.problem.room.body", {
                  needed: formatBytes(size),
                  free: formatBytes(free ?? 0),
                })}
              </p>
            </Hero>
          </Screen>
        );
      if (step.why === "missing")
        return (
          <Screen
            foot={
              <>
                <Button
                  kind="primary"
                  onClick={() => {
                    const list = nights ?? [];
                    const at = list.findIndex((n) => n.count === (night ?? list[0]?.count));
                    setPicked(list[at + 1]?.count ?? null);
                    setStep({ name: "nights" });
                  }}
                >
                  {t("phone.backup.problem.missing.action")}
                </Button>
                <button type="button" className="ph-textbtn" onClick={onBack}>
                  {t("phone.backup.back")}
                </button>
              </>
            }
          >
            <Hero icon="info" ember title={t("phone.backup.problem.missing.title")}>
              <p className="ph-body ph-ink2">{t("phone.backup.problem.missing.body")}</p>
            </Hero>
          </Screen>
        );
      if (step.why === "newer")
        return (
          <Screen
            foot={
              <Button kind="primary" onClick={onBack}>
                {t("phone.backup.ok")}
              </Button>
            }
          >
            <Hero icon="info" title={t("phone.backup.problem.newer.title")}>
              <p className="ph-body ph-ink2">
                {t("phone.backup.problem.newer.body", {
                  store: onPhone(ios, "phone.backup.store.ios", "phone.backup.store.android"),
                })}
              </p>
            </Hero>
          </Screen>
        );
      return (
        <Screen
          foot={
            <Button kind="primary" onClick={() => void connect(false)}>
              {t("phone.backup.tryAgain")}
            </Button>
          }
        >
          <Hero icon="info" title={t("phone.backup.problem.lost.title")}>
            <p className="ph-body ph-ink2">
              {onPhone(ios, "phone.backup.problem.lost.ios", "phone.backup.problem.lost.android")}
            </p>
          </Hero>
        </Screen>
      );
    }
    case "restored": {
      const { summary, kept } = step.restored;
      return (
        <Screen
          foot={
            <Button kind="primary" onClick={() => void after()}>
              {t("phone.backup.continue")}
            </Button>
          }
        >
          <Hero icon="check" title={t("phone.backup.restored.title")}>
            <p className="ph-body ph-ink2" role="status">
              {t("phone.backup.restored.body", { count: kept.things })}
            </p>
            {summary.onlineUsed ? (
              <p className="ph-small">{chips("phone.backup.restored.mind")}</p>
            ) : null}
            <p className="ph-small">{t("phone.backup.restored.sources")}</p>
          </Hero>
        </Screen>
      );
    }
    case "merged": {
      const r = step.restored;
      return (
        <Screen
          foot={
            <Button kind="primary" onClick={() => void after()}>
              {t("phone.backup.continue")}
            </Button>
          }
        >
          <Hero icon="check" title={t("phone.backup.merged.title")}>
            <p className="ph-body ph-ink2" role="status">
              {t("phone.backup.merged.body", {
                things: r.things,
                entries: r.entries,
                checkins: r.checkins,
              })}
            </p>
            {r.same > 0 ? (
              <p className="ph-small">{t("phone.backup.merged.same", { same: r.same })}</p>
            ) : null}
          </Hero>
        </Screen>
      );
    }
    case "newPhrase":
      return (
        <Screen
          foot={
            <>
              <Button kind="primary" onClick={() => setStep({ name: "phrase" })}>
                {t("phone.backup.continue")}
              </Button>
              <button
                type="button"
                className="ph-textbtn"
                onClick={() => setStep({ name: "inUse" })}
              >
                {t("phone.backup.notNow")}
              </button>
            </>
          }
        >
          <Hero icon="lock" title={t("phone.backup.newPhrase.title")}>
            <p className="ph-body ph-ink2">{t("phone.backup.newPhrase.body")}</p>
          </Hero>
        </Screen>
      );
    case "phrase":
      return (
        <PhraseStep
          draft={draft}
          onDraft={setDraft}
          newPhrase={api.newPhrase}
          onBack={() => setStep({ name: "newPhrase" })}
          onNext={() => setStep({ name: "check" })}
        />
      );
    case "check":
      return (
        <CheckStep
          draft={draft}
          onDraft={setDraft}
          size={null}
          save
          onBack={() => setStep({ name: "phrase" })}
          onDone={async (phrase) => {
            await api.changePhrase(phrase);
            setDraft(NEW_PHRASE);
            setStep({ name: "inUse" });
          }}
        />
      );
    case "inUse":
      return (
        <Screen
          foot={
            <>
              <Button
                kind="primary"
                disabled={busy}
                onClick={async () => {
                  setBusy(true);
                  await api.makeInUse().catch(() => undefined);
                  setBusy(false);
                  onDone();
                }}
              >
                {t("phone.backup.inUseAsk.yes")}
              </Button>
              <button type="button" className="ph-textbtn" onClick={onDone}>
                {t("phone.backup.notNow")}
              </button>
            </>
          }
        >
          <Hero icon="phone" title={t("phone.backup.inUseAsk.title")}>
            <p className="ph-body ph-ink2">{t("phone.backup.inUseAsk.body")}</p>
          </Hero>
        </Screen>
      );
  }
}

function WhereHeading() {
  const heading = useHeading("where");
  return (
    <h1 className="ph-steph" tabIndex={-1} ref={heading}>
      {t("phone.backup.where.title")}
    </h1>
  );
}

function FoundHeading() {
  const heading = useHeading("found");
  return (
    <h1 className="ph-steph" tabIndex={-1} ref={heading}>
      {t("phone.backup.found.title")}
    </h1>
  );
}

function MergeHeading() {
  const heading = useHeading("merge");
  return (
    <h1 className="ph-steph" tabIndex={-1} ref={heading}>
      {t("phone.backup.merge.title")}
    </h1>
  );
}
