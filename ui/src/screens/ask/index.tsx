import { useQuery } from "@tanstack/react-query";
import { type FormEvent, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useLocation } from "wouter";
import { askRequests, useAskRequest } from "../../app/ask-request";
import { hashHref } from "../../app/query";
import { BackLink } from "../../components/BackLink/BackLink";
import { Button } from "../../components/Button/Button";
import type { ChipId } from "../../components/ControlChip/registry";
import { AnswerWhere } from "../../components/KeepAskBar/AnswerWhere";
import { askPlace } from "../../components/KeepAskBar/askPlace";
import { PlaceIcon } from "../../components/KeepAskBar/PlaceIcon";
import { richText } from "../../components/PageLine/richText";
import { ReceiptChip } from "../../components/ReceiptChip/ReceiptChip";
import { ScreenSheet } from "../../components/ScreenSheet/ScreenSheet";
import { useToast } from "../../components/Toast/Toast";
import { t } from "../../copy/t";
import type { AskPhase, AskPlace, HelpView, MemoryView, Receipt } from "../../engine/bindings";
import { useAsk, useEngine, useStatus } from "../../engine/context";
import { usePlatform } from "../../platform/context";
import { useOnline, useOnlineAct } from "../online/useOnline";
import { Actions } from "./Actions";
import { AnswerMark, hasMark, spokenMark } from "./AnswerMark";
import { AnswerText } from "./AnswerText";
import styles from "./Ask.module.css";
import { ChangeNotice } from "./ChangeNotice";
import {
  DidYouMean,
  GuideReceiptCard,
  GuideReceiptChip,
  GuideWords,
  HelpButtons,
  HelpMark,
} from "./Help";
import helpStyles from "./Help.module.css";
import { ReceiptCard, receiptMeta } from "./ReceiptCard";
import { askSession, type Extras, type Snapshot, type Turn } from "./session";

const TERMINAL: ReadonlySet<string> = new Set<AskPhase>([
  "done",
  "notSure",
  "nothing",
  "unavailable",
]);

const same = (a: Receipt, b: Receipt) =>
  a.itemId === b.itemId && JSON.stringify(a.span) === JSON.stringify(b.span);

const plain = (text: string) => text.split("**").join("");

/** What stands in for an answer after some of what it used was forgotten: counts, never the words. */
const forgetNote = (gone: number, total: number) =>
  gone >= total ? t("ask.forget.done") : t("ask.forget.some", { gone, total });

/** A turn that used something now forgotten keeps its question and loses its answer. */
const scrubbed = (turn: Turn, gone: Set<string>): Turn => {
  const hit = turn.memoryIds.filter((id) => gone.has(id)).length;
  if (hit === 0) return turn;
  return {
    ...turn,
    text: "",
    receipts: [],
    memoryIds: [],
    summary: forgetNote(hit, turn.memoryIds.length),
  };
};

/** The controls the cited sections' steps name, for the written answer to draw as chips. */
function chipsOf(help: HelpView): Array<{ words: string; id: ChipId }> {
  const seen = new Map<string, ChipId>();
  for (const cite of help.sections) {
    for (const step of cite.steps) {
      for (const part of step) {
        if (part.type === "control") seen.set(part.text, part.chip as ChipId);
      }
    }
  }
  return [...seen].map(([words, id]) => ({ words, id }));
}

export default function Ask() {
  const engine = useEngine();
  const platform = usePlatform();
  const toast = useToast();
  const [, navigate] = useLocation();
  const request = useAskRequest();
  const asking = useAsk();
  const { data: status } = useStatus();
  const { data: online, isError: onlineUnread } = useOnline();
  // Until Online answers is read, a question from the bar waits, so the notice is never missed.
  const onlineRead = online !== undefined || onlineUnread;
  const host = online?.host ?? null;
  // Someone who agreed to Online answers before only what is marked private was held back sees
  // the notice before their next question goes online; the question waits for it.
  const noticeDue = useRef(false);
  noticeDue.current = online?.changeNotice === true;
  const [held, setHeld] = useState<{ question: string; place: AskPlace } | null>(null);
  const [noticeFailed, setNoticeFailed] = useState(false);
  const { run: runOnline, busy: seeing } = useOnlineAct();
  const stored = useRef(askSession.get()).current;
  const toCheck = useRef(
    new Set([
      ...stored.earlier.flatMap((turn) => turn.memoryIds),
      ...(stored.current && stored.current.extras.forgotten === null
        ? stored.current.snapshot.memoryIds
        : []),
    ]),
  ).current;
  // A stored conversation that cites memories is held back until they are confirmed to exist.
  const fresh = useRef(false);
  const seed = toCheck.size > 0 ? null : stored;
  const [current, setCurrent] = useState<string | null>(seed?.current?.question ?? null);
  const [restored, setRestored] = useState<Snapshot | null>(seed?.current?.snapshot ?? null);
  const [earlier, setEarlier] = useState<Turn[]>(seed?.earlier ?? []);
  const [extras, setExtras] = useState<Extras>(
    seed?.current?.extras ?? { forgotten: null, corrected: [] },
  );
  const [announcement, setAnnouncement] = useState("");
  const [active, setActive] = useState<string | null>(null);
  const [showOriginals, setShowOriginals] = useState(false);
  const [result, setResult] = useState("");
  const [keptAsNote, setKeptAsNote] = useState(false);
  const [followUp, setFollowUp] = useState("");
  const [followPlace, setFollowPlace] = useState<AskPlace>("auto");
  const handled = useRef<number | null>(null);
  const announced = useRef<string | null>(null);
  const turns = useRef(stored.turns);
  const heading = useRef<HTMLHeadingElement>(null);
  const focusHeading = useRef(false);
  const resultRef = useRef<HTMLParagraphElement>(null);
  const receiptsRef = useRef<HTMLDivElement>(null);

  const { deltas, error, askId } = asking;
  const liveMemoryIds = useMemo(
    () => [...deltas].reverse().find((d) => d.memoryIds.length > 0)?.memoryIds ?? [],
    [deltas],
  );
  const liveEta = useMemo(
    () => [...deltas].reverse().find((d) => d.etaSeconds !== null)?.etaSeconds ?? null,
    [deltas],
  );
  const liveBy = useMemo(
    () => [...deltas].reverse().find((d) => d.answeredBy)?.answeredBy ?? null,
    [deltas],
  );
  const liveLeftOut = useMemo(
    () => [...deltas].reverse().find((d) => d.leftOut)?.leftOut ?? null,
    [deltas],
  );
  const liveSendId = useMemo(
    () =>
      [...deltas].reverse().find((d) => d.sendId !== undefined && d.sendId !== null)?.sendId ??
      null,
    [deltas],
  );
  const liveHelp = useMemo(() => [...deltas].reverse().find((d) => d.help)?.help ?? null, [deltas]);
  const live: Snapshot = useMemo(
    () => ({
      phase: asking.phase,
      text: asking.text,
      receipts: asking.receipts,
      near: asking.near,
      found: asking.found,
      memoryIds: liveMemoryIds,
      eta: liveEta,
      failed: error !== null,
      answeredBy: liveBy,
      leftOut: liveLeftOut,
      sendId: liveSendId,
      help: liveHelp,
    }),
    [
      asking.phase,
      asking.text,
      asking.receipts,
      asking.near,
      asking.found,
      liveMemoryIds,
      liveEta,
      error,
      liveBy,
      liveLeftOut,
      liveSendId,
      liveHelp,
    ],
  );
  // Coming back to an answer shows what was there; a new question shows the live one.
  const shown = restored ?? live;
  const { phase, text, receipts, near, found, memoryIds, eta } = shown;
  const answeredBy = shown.answeredBy ?? null;
  const leftOut = shown.leftOut ?? null;
  const sendId = shown.sendId ?? null;
  const forgotten = extras.forgotten !== null;
  const help = shown.help ?? null;
  const written = help?.kind === "written" ? help : null;

  const { data: memories = [] } = useQuery({
    queryKey: ["askMemories", memoryIds.join(",")],
    enabled: memoryIds.length > 0 && !forgotten,
    queryFn: async (): Promise<MemoryView[]> => {
      const all = await engine.listMemories({
        entityId: null,
        itemId: null,
        status: null,
        query: null,
        limit: null,
      });
      return memoryIds.flatMap((id) => all.find((m) => m.id === id) ?? []);
    },
  });

  const latest = useRef({ current, shown, extras });
  latest.current = { current, shown, extras };

  const summaryOf = (now: typeof latest.current): string => {
    if (now.extras.forgotten) return now.extras.forgotten;
    switch (now.shown.phase) {
      case "done":
        return plain(now.shown.text);
      case "notSure":
        return t("ask.announce.notSure", { count: now.shown.near.length });
      case "nothing":
        return t("ask.nothing.lead");
      default:
        return t("notice.notReady.text");
    }
  };

  const start = useCallback(
    (
      question: string,
      moveFocus = false,
      place: AskPlace = "auto",
      replace = false,
      asHelp = false,
    ) => {
      if (!asHelp && place !== "thisComputer" && noticeDue.current) {
        setNoticeFailed(false);
        setHeld({ question, place });
        return;
      }
      setHeld(null);
      const now = latest.current;
      // A finished answer steps up into Earlier; one still being written is simply replaced, and
      // so is one asked again on this computer.
      if (!replace && now.current && (TERMINAL.has(now.shown.phase) || now.shown.failed)) {
        const gone = now.extras.forgotten !== null;
        const turn: Turn = {
          key: ++turns.current,
          question: now.current,
          text: gone ? "" : now.shown.text,
          summary: summaryOf(now),
          receipts: gone ? [] : now.shown.receipts,
          memoryIds: gone ? [] : now.shown.memoryIds,
          answeredBy: gone || now.shown.phase !== "done" ? null : (now.shown.answeredBy ?? null),
          sendId: now.shown.sendId ?? null,
          count: new Set(now.shown.receipts.map((r) => r.itemId)).size,
          guide:
            !gone && now.shown.phase === "done" && now.shown.help?.kind === "written"
              ? now.shown.help.sections
              : [],
        };
        setEarlier((all) => [...all, turn]);
      }
      fresh.current = true;
      focusHeading.current = moveFocus;
      setCurrent(question);
      setRestored(null);
      setExtras({ forgotten: null, corrected: [] });
      setActive(null);
      setShowOriginals(false);
      setResult("");
      setKeptAsNote(false);
      announced.current = null;
      void asking.ask(question, place, asHelp);
    },
    [asking.ask],
  );

  // After a question asked from the page itself, focus follows it to the new headline.
  useEffect(() => {
    if (focusHeading.current && current) {
      focusHeading.current = false;
      heading.current?.focus({ preventScroll: true });
    }
  }, [current]);

  // Something forgotten on another screen must not come back with a restored conversation.
  useEffect(() => {
    if (toCheck.size === 0) return;
    let cancelled = false;
    engine
      .listMemories({ entityId: null, itemId: null, status: null, query: null, limit: null })
      .then((all) => {
        if (cancelled) return;
        const present = new Set(all.map((m) => m.id));
        const gone = new Set([...toCheck].filter((id) => !present.has(id)));
        const before = stored.earlier.map((turn) => scrubbed(turn, gone));
        // A question asked meanwhile keeps the page; the checked turns go above it.
        if (fresh.current) setEarlier((now) => [...before, ...now]);
        else {
          setEarlier(before);
          if (stored.current) {
            const own = stored.current.snapshot.memoryIds;
            const hit = own.filter((id) => gone.has(id)).length;
            setCurrent(stored.current.question);
            setRestored(stored.current.snapshot);
            if (hit > 0) setResult(forgetNote(hit, own.length));
            setExtras(
              hit > 0
                ? { ...stored.current.extras, forgotten: forgetNote(hit, own.length) }
                : stored.current.extras,
            );
          }
        }
      })
      .catch(() => {
        if (cancelled) return;
        if (!fresh.current) askSession.reset();
      });
    return () => {
      cancelled = true;
    };
  }, [engine, stored, toCheck]);

  // What is on screen is what comes back after a receipt is opened.
  useEffect(() => {
    if (!current) return;
    const finished = TERMINAL.has(shown.phase) || shown.failed;
    // Once something it used is forgotten, the answer itself is not kept.
    const kept: Snapshot =
      extras.forgotten === null ? shown : { ...shown, text: "", receipts: [], memoryIds: [] };
    askSession.set({
      current: finished ? { question: current, snapshot: kept, extras } : null,
      earlier,
      turns: turns.current,
    });
  }, [current, shown, extras, earlier]);

  useEffect(() => {
    if (!request || handled.current === request.id || !onlineRead) return;
    handled.current = request.id;
    const question = askRequests.take();
    const place = askPlace.take();
    if (question) start(question, false, place);
  }, [request, start, onlineRead]);

  useEffect(() => {
    if (!current || restored) return;
    const done = TERMINAL.has(phase) || shown.failed;
    const id = askId ?? "error";
    if (!done || announced.current === id) return;
    announced.current = id;
    const marked = hasMark(answeredBy);
    if (phase === "done" && help?.kind === "written") {
      setAnnouncement(t("ask.announce.done.guide", { answer: plain(text) }));
    } else if (phase === "done") {
      setAnnouncement(
        !marked
          ? t("ask.announce.done", { answer: plain(text) })
          : answeredBy.type === "online"
            ? t("ask.announce.done.online", { host: answeredBy.host, answer: plain(text) })
            : t("ask.announce.done.local", {
                reason: spokenMark(answeredBy, host),
                answer: plain(text),
              }),
      );
    } else if (phase === "notSure")
      setAnnouncement(
        marked && answeredBy.type === "online"
          ? t("ask.announce.notSure.online", { host: answeredBy.host, count: near.length })
          : t("ask.announce.notSure", { count: near.length }),
      );
    else if (phase === "nothing") setAnnouncement(t("ask.nothing.lead"));
    else setAnnouncement(t("notice.notReady.text"));
  }, [phase, shown.failed, askId, current, restored, text, near.length, answeredBy, host, help]);

  useEffect(() => {
    if (restored) return;
    if (phase === "waitingOnline") {
      setAnnouncement(
        [
          t("ask.online.waiting.title"),
          t("ask.online.waiting.body"),
          t("ask.online.waiting.button"),
        ].join(" "),
      );
    } else if (phase === "switchingToComputer") {
      setAnnouncement(`${t("ask.online.slow")} ${t("ask.online.switching")}`);
    }
  }, [phase, restored]);

  const openReceipt = useCallback(
    (receipt: Receipt) => navigate(`/items/${receipt.itemId}`),
    [navigate],
  );

  const memoryReceipts = (memory: MemoryView | undefined): Receipt[] =>
    memory ? receipts.filter((r) => memory.receipts.some((x) => same(x, r))) : [];

  const groups = useMemo(() => {
    if (memories.length < 2) return [{ id: null as string | null, label: "", receipts }];
    const placed = new Set<Receipt>();
    const out = memories.map((m) => {
      const mine = receipts.filter((r) => m.receipts.some((x) => same(x, r)));
      for (const r of mine) placed.add(r);
      return { id: m.id as string | null, label: m.sentence, receipts: mine };
    });
    const rest = receipts.filter((r) => !placed.has(r));
    return rest.length > 0 ? [...out, { id: null, label: "", receipts: rest }] : out;
  }, [memories, receipts]);

  const isActive = (groupId: string | null, receipt: Receipt) => {
    if (active === null) return false;
    if (groupId !== null) return groupId === active;
    if (memoryIds.length === 1) return memoryIds[0] === active;
    const memory = memories.find((m) => m.id === active);
    return memoryReceipts(memory).some((r) => same(r, receipt));
  };

  const jump = (memoryId: string) => {
    const root = receiptsRef.current;
    if (!root) return;
    const group = [...root.querySelectorAll<HTMLElement>("[data-memory]")].find(
      (el) => el.dataset.memory === memoryId,
    );
    (group ?? root).querySelector<HTMLElement>("[data-open]")?.focus();
  };

  const keepPhoto = async () => {
    const paths = await platform.pickFiles();
    if (paths.length === 0) return;
    try {
      await engine.keepFiles(paths, "drop");
      toast.show({ message: t("capture.files", { count: paths.length }) });
    } catch {
      toast.show({ message: t("bar.kept.error") });
    }
  };

  const keepAsNote = async () => {
    if (!current) return;
    try {
      await engine.keepText(current, "quickCapture");
      setKeptAsNote(true);
      setAnnouncement(t("ask.unavailable.kept"));
    } catch {
      toast.show({ message: t("bar.kept.error") });
    }
  };

  const submitFollowUp = (event: FormEvent) => {
    event.preventDefault();
    const question = followUp.trim();
    if (!question) return;
    setFollowUp("");
    const place = followPlace;
    setFollowPlace("auto");
    start(question, false, place);
  };

  const finished = phase === "done" && !forgotten;
  const unavailable = phase === "unavailable" || (shown.failed && phase === "idle");
  const items = new Set(receipts.map((r) => r.itemId)).size;
  const showReceipts = receipts.length > 0 && (phase !== "waking" || showOriginals) && !forgotten;
  const internet = status?.sentMeasured === true && status.sentToInternetBytes === 0;
  const marked = hasMark(answeredBy);
  const markPhase = phase === "done" ? "done" : phase === "notSure" ? "notSure" : null;
  const onlineNow =
    phase === "askingOnline" || phase === "waitingOnline" || phase === "switchingToComputer";
  const answeredOnline = marked && answeredBy.type === "online";
  const markedCount = items > 0 ? items : memoryIds.length;
  // Only an answer still being waited for is replaced. One that was sent and counted stays in
  // Earlier, and the answer on this computer follows it as a new turn.
  const askHere = () => {
    if (!current) return;
    const waiting = latest.current.shown.phase === "waitingOnline";
    heading.current?.focus({ preventScroll: true });
    start(current, false, "thisComputer", waiting);
  };
  // Seen: the notice never comes back, and the question that waited goes on its way.
  const continueHeld = async () => {
    if (!held) return;
    setNoticeFailed(false);
    const done = await runOnline(() => engine.seeOnlineChange(), { quiet: true });
    if (!done) {
      setNoticeFailed(true);
      return;
    }
    noticeDue.current = false;
    start(held.question, true, held.place);
  };
  const memoriesHref = hashHref("/memories", { q: current });
  // "Show the guide's answer": the same question again, answered from the guide, in place.
  const askGuide = () => {
    if (!current) return;
    heading.current?.focus({ preventScroll: true });
    start(current, false, "auto", true, true);
  };
  const helpDone = written !== null && phase === "done" && !forgotten;
  const words = help?.kind === "words" && TERMINAL.has(phase) ? help : null;

  const onForgotten = (ids: string[]) => {
    const note = forgetNote(memoryIds.filter((id) => ids.includes(id)).length, memoryIds.length);
    setExtras((e) => ({ ...e, forgotten: note }));
    // The forgotten claim must not linger in what a screen reader holds or in earlier turns.
    setAnnouncement("");
    setEarlier((all) => all.map((turn) => scrubbed(turn, new Set(ids))));
    setResult(note);
  };

  return (
    <ScreenSheet>
      <BackLink to="/today" label={t("screen.today.title")} />
      <div className={styles.column}>
        {finished && items > 0 && !marked && !helpDone ? (
          <p className={styles.provenance}>
            {t(internet ? "ask.provenance" : "ask.provenance.plain", { count: items })}
          </p>
        ) : null}
        {earlier.length > 0 ? (
          <section className={styles.earlier} aria-labelledby="ask-earlier">
            <p id="ask-earlier" className={styles.eyebrow}>
              {t("ask.earlier")}
            </p>
            <ol className={styles.turns}>
              {earlier.map((turn) => (
                <li key={turn.key} className={styles.turn}>
                  {turn.answeredBy && hasMark(turn.answeredBy) && turn.text ? (
                    <AnswerMark
                      by={turn.answeredBy}
                      phase="done"
                      count={turn.count ?? turn.memoryIds.length}
                      host={host}
                      sendId={turn.sendId ?? null}
                    />
                  ) : null}
                  <p className={styles.turnQuestion}>
                    {t("ask.question", { question: turn.question })}
                  </p>
                  <p className={styles.turnAnswer}>
                    {turn.text ? richText(turn.text) : turn.summary}
                  </p>
                  {turn.receipts.length > 0 || (turn.guide?.length ?? 0) > 0 ? (
                    <span className={styles.chips}>
                      {turn.guide?.map((cite) => (
                        <GuideReceiptChip key={cite.id} cite={cite} />
                      ))}
                      {turn.receipts.map((r) => (
                        <ReceiptChip
                          key={`${r.itemId}-${JSON.stringify(r.span)}`}
                          receipt={r}
                          onOpen={openReceipt}
                        />
                      ))}
                    </span>
                  ) : null}
                </li>
              ))}
            </ol>
          </section>
        ) : null}

        {held ? (
          <ChangeNotice
            question={held.question}
            busy={seeing}
            failed={noticeFailed}
            onContinue={() => void continueHeld()}
            onSettings={() => {
              setHeld(null);
              navigate("/settings/online");
            }}
          />
        ) : null}
        {current && !held ? (
          <article className={styles.turnNow} aria-busy={!TERMINAL.has(phase) && !shown.failed}>
            {helpDone && written ? (
              <HelpMark help={written} count={items} />
            ) : marked && markPhase && !forgotten ? (
              <AnswerMark
                by={answeredBy}
                phase={markPhase}
                count={markedCount}
                host={host}
                sendId={sendId}
              />
            ) : null}
            <p className={styles.eyebrow}>{t("ask.eyebrow")}</p>
            <h1 ref={heading} tabIndex={-1} className={`t-page-title ${styles.question}`}>
              {t("ask.question", { question: current })}
            </h1>

            <div className={styles.body}>
              {words ? <GuideWords help={words} /> : null}
              {words ? (
                <p className={`${styles.eyebrow} ${helpStyles.keptHead}`}>{t("ask.help.kept")}</p>
              ) : null}
              {phase === "waking" ? (
                <div className={styles.waking} role="status">
                  <p className={styles.wakingTitle}>
                    <span className={styles.dot} aria-hidden="true" />
                    {t("ask.waking.title")}
                  </p>
                  <p className={styles.wakingBody}>
                    {t("ask.waking.body", { seconds: eta ?? 20 })}
                  </p>
                  {found !== null ? (
                    <div className={styles.progress}>
                      <p>
                        <span aria-hidden="true">✓ </span>
                        {t("ask.found", { count: found })}
                      </p>
                      <p className={styles.reading}>
                        <span aria-hidden="true">· </span>
                        {t("ask.reading", { count: found })}
                      </p>
                    </div>
                  ) : null}
                  {receipts.length > 0 && !showOriginals ? (
                    <button
                      type="button"
                      className={styles.link}
                      onClick={() => setShowOriginals(true)}
                    >
                      {t("ask.showOriginals", { count: receipts.length })}
                    </button>
                  ) : null}
                </div>
              ) : null}

              {onlineNow ? (
                <>
                  <ul className={styles.phases} role="status">
                    <li>
                      <span aria-hidden="true" className={styles.phaseGlyph}>
                        ✓
                      </span>
                      {t("ask.online.looked")}
                    </li>
                    {found !== null ? (
                      <li>
                        <span aria-hidden="true" className={styles.phaseGlyph}>
                          ✓
                        </span>
                        {t("ask.online.found", { count: found })}
                      </li>
                    ) : null}
                    {phase === "switchingToComputer" ? (
                      <>
                        <li>
                          <span aria-hidden="true" className={styles.phaseGlyph}>
                            –
                          </span>
                          {t("ask.online.slow")}
                        </li>
                        <li data-now="true">
                          <span aria-hidden="true" className={styles.phaseGlyph}>
                            <span className={styles.dot} />
                          </span>
                          {t("ask.online.switching")}
                        </li>
                      </>
                    ) : (
                      <li data-now="true">
                        <span aria-hidden="true" className={styles.phaseGlyph}>
                          <span className={styles.dot} />
                        </span>
                        {host ? t("ask.online.asking", { host }) : t("ask.online.asking.unknown")}
                      </li>
                    )}
                  </ul>
                  {phase === "waitingOnline" ? (
                    <div className={styles.waitNote}>
                      <span className={styles.waitText}>
                        <span className={styles.waitTitle}>{t("ask.online.waiting.title")}</span>
                        <span className={styles.waitHint}>{t("ask.online.waiting.body")}</span>
                      </span>
                      <Button variant="primary" onClick={askHere}>
                        {t("ask.online.waiting.button")}
                      </Button>
                    </div>
                  ) : null}
                </>
              ) : null}

              {phase === "asking" || phase === "searching" ? (
                <p className={styles.searching}>
                  <span className={styles.dot} aria-hidden="true" />
                  {t("ask.searching")}
                </p>
              ) : null}

              {phase === "reading" && found !== null ? (
                <div className={styles.progress}>
                  <p>
                    <span aria-hidden="true">✓ </span>
                    {t("ask.found", { count: found })}
                  </p>
                  <p className={styles.reading}>
                    <span aria-hidden="true">· </span>
                    {t("ask.reading", { count: found })}
                  </p>
                </div>
              ) : null}

              {(phase === "streaming" || phase === "done") && !forgotten ? (
                <AnswerText
                  text={text}
                  memoryIds={written ? written.cited : memoryIds}
                  chips={written ? chipsOf(written) : undefined}
                  corrected={extras.corrected.length > 0}
                  onActive={setActive}
                  onJump={jump}
                />
              ) : null}
              {!forgotten
                ? extras.corrected.map((sentence) => (
                    <p key={sentence} className={styles.corrected}>
                      {t("ask.corrected", { sentence })}
                    </p>
                  ))
                : null}

              {phase === "notSure" ? (
                <>
                  <p className={styles.lead}>
                    {answeredOnline
                      ? t("ask.notSure.online.lead", { count: near.length })
                      : t("ask.notSure.lead", { count: near.length })}
                  </p>
                  <ul className={styles.near}>
                    {near.map((match) => (
                      <li key={`${match.receipt.itemId}-${match.text}`}>
                        <button
                          type="button"
                          className={styles.nearCard}
                          onClick={() => openReceipt(match.receipt)}
                        >
                          <span className={styles.nearText}>{match.text}</span>
                          <span className={styles.receiptMeta}>{receiptMeta(match.receipt)}</span>
                        </button>
                      </li>
                    ))}
                  </ul>
                  {answeredOnline ? (
                    <div className={styles.buttons}>
                      <Button variant="primary" onClick={askHere}>
                        {t("ask.leftOut.button")}
                      </Button>
                      <Button
                        variant="secondary"
                        onClick={() => {
                          window.location.hash = memoriesHref;
                        }}
                      >
                        {t("ask.notSure.search")}
                      </Button>
                    </div>
                  ) : (
                    <a className={styles.linkAnchor} href={memoriesHref}>
                      {t("ask.notSure.search")}
                    </a>
                  )}
                </>
              ) : null}

              {phase === "nothing" && words ? (
                <p className={styles.note}>{t("ask.help.kept.nothing")}</p>
              ) : null}

              {phase === "nothing" && !words ? (
                <>
                  <p className={styles.lead}>{t("ask.nothing.lead")}</p>
                  <p className={styles.note}>{t("ask.nothing.body")}</p>
                  <div className={styles.buttons}>
                    <Button variant="primary" onClick={() => void keepPhoto()}>
                      {t("ask.nothing.keepPhoto")}
                    </Button>
                    <Button
                      variant="secondary"
                      onClick={() => {
                        window.location.hash = hashHref("/fading", { show: "letGo" });
                      }}
                    >
                      {t("ask.nothing.letGo")}
                    </Button>
                  </div>
                </>
              ) : null}

              {unavailable ? (
                <>
                  <p className={styles.lead}>{t("notice.notReady.text")}</p>
                  {keptAsNote ? (
                    <p className={styles.note}>{t("ask.unavailable.kept")}</p>
                  ) : (
                    <div className={styles.buttons}>
                      <Button variant="secondary" onClick={() => void keepAsNote()}>
                        {t("notice.notReady.action")}
                      </Button>
                    </div>
                  )}
                </>
              ) : null}

              {helpDone && written ? (
                <div ref={receiptsRef} className={styles.receipts}>
                  {written.sections.length > 0 ? (
                    <ul className={styles.group} aria-label={t("ask.help.receipts")}>
                      {written.sections.map((cite) => (
                        <GuideReceiptCard key={cite.id} cite={cite} active={active === cite.id} />
                      ))}
                    </ul>
                  ) : null}
                  {receipts.length > 0
                    ? groups.map((group) => (
                        <ul
                          key={group.id ?? "all"}
                          className={styles.group}
                          data-memory={group.id ?? undefined}
                          aria-label={
                            group.label
                              ? t("ask.receipt.groupFor", { sentence: group.label })
                              : t("receipt.group")
                          }
                        >
                          {group.receipts.map((receipt) => (
                            <ReceiptCard
                              key={`${receipt.itemId}-${JSON.stringify(receipt.span)}`}
                              receipt={receipt}
                              active={isActive(group.id, receipt)}
                              onOpen={openReceipt}
                            />
                          ))}
                        </ul>
                      ))
                    : null}
                  <HelpButtons help={written} />
                </div>
              ) : showReceipts ? (
                <div ref={receiptsRef} className={styles.receipts}>
                  {groups.map((group) => (
                    <ul
                      key={group.id ?? "all"}
                      className={styles.group}
                      data-memory={group.id ?? undefined}
                      aria-label={
                        group.label
                          ? t("ask.receipt.groupFor", { sentence: group.label })
                          : t("receipt.group")
                      }
                    >
                      {group.receipts.map((receipt) => (
                        <ReceiptCard
                          key={`${receipt.itemId}-${JSON.stringify(receipt.span)}`}
                          receipt={receipt}
                          active={isActive(group.id, receipt)}
                          onOpen={openReceipt}
                        />
                      ))}
                    </ul>
                  ))}
                </div>
              ) : null}

              {leftOut &&
              answeredOnline &&
              (phase === "done" || phase === "notSure") &&
              !forgotten ? (
                <div className={styles.leftOut}>
                  <span className={styles.leftOutText}>
                    <PlaceIcon place="computer" size={16} />
                    {t(leftOut === "private" ? "ask.leftOut.private" : "ask.leftOut.other")}
                  </span>
                  {phase === "done" ? (
                    <Button variant="secondary" onClick={askHere}>
                      {t("ask.leftOut.button")}
                    </Button>
                  ) : null}
                </div>
              ) : null}

              {help?.kind === "unsure" && TERMINAL.has(phase) && !forgotten ? (
                <DidYouMean help={help} onShow={askGuide} />
              ) : null}

              {phase === "done" ? (
                <Actions
                  memories={forgotten ? [] : memories}
                  showButtons={!forgotten && !(written && memoryIds.length === 0)}
                  result={result}
                  onResult={setResult}
                  onCorrected={(sentence) =>
                    setExtras((e) => ({ ...e, corrected: [...e.corrected, sentence] }))
                  }
                  onForgotten={onForgotten}
                  resultRef={resultRef}
                />
              ) : null}
            </div>
          </article>
        ) : held ? null : (
          <EmptyAsk onAsk={(question) => start(question, true)} />
        )}
      </div>

      <div className="sr-only" role="status" aria-live="polite" aria-atomic="true">
        {announcement}
      </div>

      <form className={styles.followUp} onSubmit={submitFollowUp}>
        <AnswerWhere place={followPlace} onPlace={setFollowPlace} className={styles.followWhere} />
        <div className={styles.followField}>
          <input
            className={styles.followInput}
            type="text"
            value={followUp}
            aria-label={t("ask.followup.label")}
            placeholder={t("ask.followup.placeholder")}
            onChange={(event) => setFollowUp(event.target.value)}
          />
          <Button type="submit" variant="secondary" disabled={followUp.trim() === ""}>
            {t("ask.followup.send")}
          </Button>
        </div>
      </form>
    </ScreenSheet>
  );
}

function EmptyAsk({ onAsk }: { onAsk: (question: string) => void }) {
  const EXAMPLES = [t("ask.example.1"), t("ask.example.2"), t("ask.example.3"), t("ask.example.4")];
  return (
    <section className={styles.empty}>
      <h1 className={`t-page-title ${styles.question}`}>{t("ask.empty.title")}</h1>
      <p className={styles.note}>{t("ask.empty.body")}</p>
      <p className={styles.eyebrow}>{t("ask.empty.examples")}</p>
      <ul className={styles.examples}>
        {EXAMPLES.map((question) => (
          <li key={question}>
            <button type="button" className={styles.example} onClick={() => onAsk(question)}>
              {question}
            </button>
          </li>
        ))}
      </ul>
    </section>
  );
}
