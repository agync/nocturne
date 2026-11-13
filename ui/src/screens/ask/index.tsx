import { useQuery } from "@tanstack/react-query";
import { type FormEvent, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useLocation } from "wouter";
import { askRequests, useAskRequest } from "../../app/ask-request";
import { hashHref } from "../../app/query";
import { BackLink } from "../../components/BackLink/BackLink";
import { Button } from "../../components/Button/Button";
import { richText } from "../../components/PageLine/richText";
import { ReceiptChip } from "../../components/ReceiptChip/ReceiptChip";
import { ScreenSheet } from "../../components/ScreenSheet/ScreenSheet";
import { useToast } from "../../components/Toast/Toast";
import { t } from "../../copy/t";
import type { AskPhase, MemoryView, Receipt } from "../../engine/bindings";
import { useAsk, useEngine, useStatus } from "../../engine/context";
import { usePlatform } from "../../platform/context";
import { Actions } from "./Actions";
import { AnswerText } from "./AnswerText";
import styles from "./Ask.module.css";
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

export default function Ask() {
  const engine = useEngine();
  const platform = usePlatform();
  const toast = useToast();
  const [, navigate] = useLocation();
  const request = useAskRequest();
  const asking = useAsk();
  const { data: status } = useStatus();
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
    ],
  );
  // Coming back to an answer shows what was there; a new question shows the live one.
  const shown = restored ?? live;
  const { phase, text, receipts, near, found, memoryIds, eta } = shown;
  const forgotten = extras.forgotten !== null;

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
    (question: string, moveFocus = false) => {
      const now = latest.current;
      // A finished answer steps up into Earlier; one still being written is simply replaced.
      if (now.current && (TERMINAL.has(now.shown.phase) || now.shown.failed)) {
        const gone = now.extras.forgotten !== null;
        const turn: Turn = {
          key: ++turns.current,
          question: now.current,
          text: gone ? "" : now.shown.text,
          summary: summaryOf(now),
          receipts: gone ? [] : now.shown.receipts,
          memoryIds: gone ? [] : now.shown.memoryIds,
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
      void asking.ask(question);
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
    if (!request || handled.current === request.id) return;
    handled.current = request.id;
    const question = askRequests.take();
    if (question) start(question);
  }, [request, start]);

  useEffect(() => {
    if (!current || restored) return;
    const done = TERMINAL.has(phase) || shown.failed;
    const id = askId ?? "error";
    if (!done || announced.current === id) return;
    announced.current = id;
    if (phase === "done") setAnnouncement(t("ask.announce.done", { answer: plain(text) }));
    else if (phase === "notSure")
      setAnnouncement(t("ask.announce.notSure", { count: near.length }));
    else if (phase === "nothing") setAnnouncement(t("ask.nothing.lead"));
    else setAnnouncement(t("notice.notReady.text"));
  }, [phase, shown.failed, askId, current, restored, text, near.length]);

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
    (group ?? root).querySelector<HTMLElement>("button[data-open]")?.focus();
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
    start(question);
  };

  const finished = phase === "done" && !forgotten;
  const unavailable = phase === "unavailable" || (shown.failed && phase === "idle");
  const items = new Set(receipts.map((r) => r.itemId)).size;
  const showReceipts = receipts.length > 0 && (phase !== "waking" || showOriginals) && !forgotten;
  const internet = status?.sentMeasured === true && status.sentToInternetBytes === 0;
  const memoriesHref = hashHref("/memories", { q: current });

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
        {finished && items > 0 ? (
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
                  <p className={styles.turnQuestion}>
                    {t("ask.question", { question: turn.question })}
                  </p>
                  <p className={styles.turnAnswer}>
                    {turn.text ? richText(turn.text) : turn.summary}
                  </p>
                  {turn.receipts.length > 0 ? (
                    <span className={styles.chips}>
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

        {current ? (
          <article className={styles.turnNow} aria-busy={!TERMINAL.has(phase) && !shown.failed}>
            <p className={styles.eyebrow}>{t("ask.eyebrow")}</p>
            <h1 ref={heading} tabIndex={-1} className={`t-page-title ${styles.question}`}>
              {t("ask.question", { question: current })}
            </h1>

            <div className={styles.body}>
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

              {phase === "asking" ||
              phase === "searching" ||
              phase === "askingOnline" ||
              phase === "waitingOnline" ||
              phase === "switchingToComputer" ? (
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
                  memoryIds={memoryIds}
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
                  <p className={styles.lead}>{t("ask.notSure.lead", { count: near.length })}</p>
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
                  <a className={styles.linkAnchor} href={memoriesHref}>
                    {t("ask.notSure.search")}
                  </a>
                </>
              ) : null}

              {phase === "nothing" ? (
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

              {showReceipts ? (
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

              {phase === "done" ? (
                <Actions
                  memories={forgotten ? [] : memories}
                  showButtons={!forgotten}
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
        ) : (
          <EmptyAsk onAsk={(question) => start(question, true)} />
        )}
      </div>

      <div className="sr-only" role="status" aria-live="polite" aria-atomic="true">
        {announcement}
      </div>

      <form className={styles.followUp} onSubmit={submitFollowUp}>
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
  const EXAMPLES = [t("ask.example.1"), t("ask.example.2"), t("ask.example.3")];
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
