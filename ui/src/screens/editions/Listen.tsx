import { useQuery } from "@tanstack/react-query";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { useRoute } from "wouter";
import { Button } from "../../components/Button/Button";
import { formatLongDay } from "../../copy/format";
import { type CopyId, t } from "../../copy/t";
import type { PageView, SpokenKind, SpokenSection } from "../../engine/bindings";
import { EngineError } from "../../engine/client";
import { useEngine, useRefresh, useSettings } from "../../engine/context";
import { usePlatform } from "../../platform/context";
import { patch } from "../settings/patch";
import { voiceName } from "../settings/voices";
import styles from "./Listen.module.css";
import { usePlayer } from "./usePlayer";

const clock = (seconds: number): string => {
  const whole = Math.max(0, Math.floor(seconds));
  return `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, "0")}`;
};

/** How many lines of the page each section reads, for the count beside its name. */
function counts(page: PageView): Partial<Record<SpokenKind, number>> {
  const live = (n: PageView["needsYou"]) => n.filter((l) => !l.forgotten).length;
  return {
    needsYou: live(page.needsYou),
    changed: Math.min(6, live(page.changed)),
    comingUp: Math.min(6, live(page.upcoming)),
    letGo: live(page.letGo),
  };
}

function sectionLabel(kind: SpokenKind, page: PageView): string {
  const label = t(`listen.section.${kind}` as CopyId);
  const count = counts(page)[kind];
  return kind === "changed" || kind === "comingUp" || kind === "letGo"
    ? count
      ? t("listen.section.count", { label, count })
      : label
    : label;
}

function failure(error: unknown): { text: string; again: boolean } {
  const code = error instanceof EngineError ? error.code : "";
  if (code === "unavailable") return { text: t("listen.unavailable"), again: true };
  if (code === "notFound") return { text: t("listen.missing"), again: false };
  if (code === "busy") return { text: t("listen.busy"), again: true };
  return { text: t("listen.failed"), again: true };
}

/**
 * Whether the player leaves this key to what has focus: text boxes keep every key, and a button
 * or link keeps the keys that activate it. Everything else is the player's.
 */
const leaves = (target: EventTarget | null, key: string): boolean => {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  const tag = target.tagName;
  if (tag === "TEXTAREA" || tag === "SELECT") return true;
  if (tag === "BUTTON" || tag === "A") return key === " " || key === "Enter";
  return tag === "INPUT" && (target as HTMLInputElement).type !== "range";
};

interface PlayerViewProps {
  page: PageView;
  path: string;
  sections: SpokenSection[];
  durationMs: number;
  privateSkipped: number;
  voice: string;
  rate: number;
  onRate(rate: number): void;
}

function PlayerView({
  page,
  path,
  sections,
  durationMs,
  privateSkipped,
  voice,
  rate,
  onRate,
}: PlayerViewProps) {
  const platform = usePlatform();
  const audio = useRef<HTMLAudioElement>(null);
  const body = useRef<HTMLDivElement>(null);
  const player = usePlayer({ audio, sections, durationMs, rate, onRate });
  const [announcement, setAnnouncement] = useState("");

  // Only a change of section is announced: the time ticking is not.
  const announced = useRef(-1);
  const { index, onKey } = player;
  useEffect(() => {
    if (!player.playing || announced.current === index) return;
    announced.current = index;
    const kind = sections[index]?.kind;
    if (kind) setAnnouncement(t("listen.now", { section: sectionLabel(kind, page) }));
  }, [index, player.playing, sections, page]);

  // Focus starts on the screen itself, not on a text box, so the keys work at once.
  useEffect(() => {
    body.current?.focus({ preventScroll: true });
  }, []);

  useEffect(() => {
    const listener = (event: KeyboardEvent) => {
      if (leaves(event.target, event.key)) return;
      onKey(event);
    };
    window.addEventListener("keydown", listener);
    return () => window.removeEventListener("keydown", listener);
  }, [onKey]);

  const src = path.startsWith("blob:") ? path : platform.originalSrc(path);
  const ends = sections.map(
    (s, i) => (sections[i + 1]?.startMs ?? durationMs) / 1000 - 0 * s.startMs,
  );

  return (
    <div ref={body} className={styles.body} tabIndex={-1}>
      <header className={styles.head}>
        <p className={styles.eyebrow}>
          {t("listen.eyebrow", { day: formatLongDay(page.date), voice })}
        </p>
        <h1 className={styles.headline}>{page.headline}</h1>
      </header>
      {/* biome-ignore lint/a11y/useMediaCaption: the transcript is the page itself, shown below */}
      <audio ref={audio} src={src} preload="auto" />
      <ol className={styles.sections} aria-label={t("listen.sections")}>
        {sections.map((s, i) => (
          <li
            key={`${s.kind}-${s.startMs}`}
            className={styles.section}
            aria-current={i === player.index ? "true" : undefined}
          >
            <button type="button" className={styles.jump} onClick={() => player.jump(i)}>
              <span className={styles.time}>{clock(s.startMs / 1000)}</span>
              <span className={styles.name}>{sectionLabel(s.kind, page)}</span>
              <span className={styles.time}>{clock(ends[i] ?? 0)}</span>
            </button>
          </li>
        ))}
      </ol>
      {privateSkipped > 0 ? (
        <p className={styles.skipped}>{t("listen.skipped", { count: privateSkipped })}</p>
      ) : null}
      <section className={styles.player} aria-label={t("listen.player")}>
        <div className={styles.scrub}>
          <span className={styles.time} aria-hidden="true">
            {clock(player.time)}
          </span>
          <div className={styles.track}>
            <input
              className={styles.range}
              type="range"
              min={0}
              max={Math.max(1, Math.round(player.duration))}
              step={1}
              value={Math.round(player.time)}
              aria-label={t("listen.position")}
              aria-valuetext={t("listen.position.value", {
                time: clock(player.time),
                total: clock(player.duration),
              })}
              onChange={(event) => player.seek(Number(event.target.value))}
            />
            <div className={styles.marks} aria-hidden="true">
              {sections.slice(1).map((s) => (
                <span
                  key={s.startMs}
                  className={styles.mark}
                  style={{ left: `${(s.startMs / Math.max(1, player.duration * 1000)) * 100}%` }}
                />
              ))}
            </div>
          </div>
          <span className={styles.time} aria-hidden="true">
            {clock(player.duration)}
          </span>
        </div>
        <div className={styles.controls}>
          <Button variant="quiet" onClick={player.previous}>
            {t("listen.previous")}
          </Button>
          <Button variant="primary" onClick={player.toggle} aria-pressed={player.playing}>
            {player.playing ? t("listen.pause") : t("listen.play")}
          </Button>
          <Button variant="quiet" onClick={player.next}>
            {t("listen.next")}
          </Button>
          <span className={styles.speed}>
            <Button variant="quiet" onClick={player.slower} aria-label={t("listen.slower")}>
              −
            </Button>
            <output className={styles.rate} aria-label={t("listen.speed.label")}>
              {t("listen.speed.value", { speed: player.rate })}
            </output>
            <Button variant="quiet" onClick={player.faster} aria-label={t("listen.faster")}>
              +
            </Button>
          </span>
        </div>
        <p className={styles.keys}>{t("listen.keys")}</p>
      </section>
      <p className="sr-only" role="status" aria-live="polite">
        {announcement}
      </p>
    </div>
  );
}

function Frame({ children }: { children: ReactNode }) {
  return (
    <div className={styles.frame}>
      <div className={styles.card}>{children}</div>
    </div>
  );
}

/** Serves #/listen/:date: the page read aloud, in sections, with a player. */
export function Listen() {
  const [, params] = useRoute("/listen/:date");
  const date = params?.date ?? "";
  const engine = useEngine();
  const refresh = useRefresh();
  const settings = useSettings();

  const page = useQuery({
    queryKey: ["edition", date, "audio"],
    queryFn: () => engine.getEdition(date, "audio"),
    gcTime: 0,
  });
  // Asked every time the screen opens, and never kept: if something on the page has become
  // private since, the engine makes the audio again and this never plays an old copy.
  const spoken = useQuery({
    queryKey: ["spoken", date],
    queryFn: () => engine.speakPage(date),
    gcTime: 0,
    staleTime: 0,
  });

  if (spoken.isError || page.isError) {
    const { text, again } = failure(spoken.error ?? page.error);
    return (
      <Frame>
        <h1 className="t-page-title">{t("screen.listen.title")}</h1>
        <p className={styles.message} role="alert">
          {text}
        </p>
        <div className={styles.actions}>
          {again ? (
            <Button
              variant="secondary"
              onClick={() => {
                void spoken.refetch();
                void page.refetch();
              }}
            >
              {t("button.tryAgain")}
            </Button>
          ) : null}
          <a className={styles.back} href={`#/pages/${date}`}>
            {t("listen.read")}
          </a>
        </div>
      </Frame>
    );
  }
  if (!spoken.data || !page.data || !settings.data) {
    return (
      <Frame>
        <h1 className="t-page-title">{t("screen.listen.title")}</h1>
        <p className={styles.message} role="status">
          {t("listen.waiting")}
        </p>
      </Frame>
    );
  }

  const voice = voiceName(settings.data.voice).replace(/ \(.*\)$/, "");
  return (
    <Frame>
      <PlayerView
        key={spoken.data.path}
        page={page.data}
        path={spoken.data.path}
        sections={spoken.data.sections}
        durationMs={spoken.data.durationMs}
        privateSkipped={spoken.data.privateSkipped}
        voice={voice}
        rate={settings.data.voiceSpeed}
        onRate={(voiceSpeed) => {
          void engine
            .updateSettings(patch({ voiceSpeed }))
            .then(() => refresh("settings"))
            .catch(() => undefined);
        }}
      />
    </Frame>
  );
}
