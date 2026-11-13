import { useQuery } from "@tanstack/react-query";
import { type FormEvent, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useLocation } from "wouter";
import { askRequests, useAskRequest } from "../../app/ask-request";
import { hashHref } from "../../app/query";
import { Button } from "../../components/Button/Button";
import { richText } from "../../components/PageLine/richText";
import { ReceiptChip } from "../../components/ReceiptChip/ReceiptChip";
import { useToast } from "../../components/Toast/Toast";
import { formatTime } from "../../copy/format";
import { t } from "../../copy/t";
import type { AskPhase, MemoryView, Receipt } from "../../engine/bindings";
import { useAsk, useEngine, useStatus } from "../../engine/context";
import { usePlatform } from "../../platform/context";
import { Actions } from "./Actions";
import { AnswerText } from "./AnswerText";
import styles from "./Ask.module.css";
import { ReceiptCard, receiptMeta } from "./ReceiptCard";
import { topicOf } from "./topic";

const TERMINAL: ReadonlySet<string> = new Set<AskPhase>([
  "done",
  "notSure",
  "nothing",
  "unavailable",
]);

const same = (a: Receipt, b: Receipt) =>
  a.itemId === b.itemId && JSON.stringify(a.span) === JSON.stringify(b.span);

/** A finished turn, kept on screen above the new question. */
interface Turn {
  key: number;
  question: string;
  text: string;
  summary: string;
  receipts: Receipt[];
}

const plain = (text: string) => text.split("**").join("");

export default function Ask() {
  const engine = useEngine();
  const platform = usePlatform();
  const toast = useToast();
  const [, navigate] = useLocation();
  const request = useAskRequest();
  const asking = useAsk();
  const { data: status } = useStatus();
  const [current, setCurrent] = useState<{ question: string; time: string } | null>(null);
  const [earlier, setEarlier] = useState<Turn[]>([]);
  const [announcement, setAnnouncement] = useState("");
  const [active, setActive] = useState<string | null>(null);
  const [showOriginals, setShowOriginals] = useState(false);
  const [result, setResult] = useState("");
  const [forgotten, setForgotten] = useState(false);
  const [keptAsNote, setKeptAsNote] = useState(false);
  const [followUp, setFollowUp] = useState("");
  const handled = useRef<number | null>(null);
  const announced = useRef<string | null>(null);
  const turns = useRef(0);
  const resultRef = useRef<HTMLParagraphElement>(null);
  const receiptsRef = useRef<HTMLDivElement>(null);

  const { phase, text, receipts, near, found, deltas, error, askId } = asking;
  const memoryIds = useMemo(
    () => [...deltas].reverse().find((d) => d.memoryIds.length > 0)?.memoryIds ?? [],
    [deltas],
  );
  const eta = useMemo(
    () => [...deltas].reverse().find((d) => d.etaSeconds !== null)?.etaSeconds ?? null,
    [deltas],
  );

  const { data: memories = [] } = useQuery({
    queryKey: ["askMemories", memoryIds.join(",")],
    enabled: memoryIds.length > 0,
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

  const latest = useRef({ current, phase, text, receipts, error, forgotten });
  latest.current = { current, phase, text, receipts, error, forgotten };

  const summaryOf = useCallback((question: string): string => {
    const now = latest.current;
    if (now.phase === "done") return plain(now.text);
    if (now.phase === "notSure") return t("ask.notSure.lead", { count: 2 });
    if (now.phase === "nothing") return t("ask.nothing.lead", { topic: topicOf(question) });
    return t("notice.notReady.text");
  }, []);

  const start = useCallback(
    (question: string) => {
      const now = latest.current;
      // A finished answer steps up into Earlier; one still being written is simply replaced.
      if (now.current && (TERMINAL.has(now.phase) || now.error)) {
        const turn: Turn = {
          key: ++turns.current,
          question: now.current.question,
          text: now.forgotten ? "" : now.text,
          summary: now.forgotten ? t("ask.forget.done") : summaryOf(now.current.question),
          receipts: now.forgotten ? [] : now.receipts,
        };
        setEarlier((all) => [...all, turn]);
      }
      setCurrent({ question, time: formatTime(new Date()) });
      setActive(null);
      setShowOriginals(false);
      setResult("");
      setForgotten(false);
      setKeptAsNote(false);
      announced.current = null;
      void asking.ask(question);
    },
    [asking.ask, summaryOf],
  );

  useEffect(() => {
    if (!request || handled.current === request.id) return;
    handled.current = request.id;
    const question = askRequests.take();
    if (question) start(question);
  }, [request, start]);

  useEffect(() => {
    if (!current) return;
    const done = TERMINAL.has(phase) || error !== null;
    const id = askId ?? "error";
    if (!done || announced.current === id) return;
    announced.current = id;
    if (phase === "done") setAnnouncement(t("ask.announce.done", { answer: plain(text) }));
    else if (phase === "notSure")
      setAnnouncement(t("ask.announce.notSure", { count: near.length }));
    else if (phase === "nothing")
      setAnnouncement(t("ask.announce.nothing", { topic: topicOf(current.question) }));
    else setAnnouncement(t("notice.notReady.text"));
  }, [phase, error, askId, current, text, near.length]);

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
      await engine.keepText(current.question, "quickCapture");
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
  const unavailable = phase === "unavailable" || (error !== null && phase === "idle");
  const items = new Set(receipts.map((r) => r.itemId)).size;
  const showReceipts = receipts.length > 0 && (phase !== "waking" || showOriginals) && !forgotten;
  const internet = status?.sentMeasured === true && status.sentToInternetBytes === 0;
  const topic = current ? topicOf(current.question) : "";

  return (
    <div className={styles.screen}>
      <div className={styles.bar}>
        <a className={styles.back} href="#/today" aria-label={t("ask.back.label")}>
          <span className={styles.arrow} aria-hidden="true">
            ←
          </span>
          {t("ask.back")}
        </a>
        {finished && items > 0 ? (
          <span className={styles.provenance}>
            {t(internet ? "ask.provenance" : "ask.provenance.plain", { count: items })}
          </span>
        ) : null}
      </div>

      <div className={styles.column}>
        {earlier.length > 0 ? (
          <section className={styles.earlier} aria-labelledby="ask-earlier">
            <h2 id="ask-earlier" className={styles.eyebrow}>
              {t("ask.earlier")}
            </h2>
            <ol className={styles.turns}>
              {earlier.map((turn) => (
                <li key={turn.key} className={styles.turn}>
                  <h3 className={styles.turnQuestion}>
                    {t("ask.question", { question: turn.question })}
                  </h3>
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
          <article className={styles.turnNow} aria-busy={!TERMINAL.has(phase) && error === null}>
            <p className={styles.eyebrow}>{t("ask.eyebrow", { time: current.time })}</p>
            <h1 className={styles.question}>{t("ask.question", { question: current.question })}</h1>

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
                <AnswerText text={text} memoryIds={memoryIds} onActive={setActive} onJump={jump} />
              ) : null}
              {forgotten ? <p className={styles.lead}>{t("ask.forget.done")}</p> : null}

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
                  <a className={styles.linkAnchor} href={hashHref("/memories", { q: topic })}>
                    {t("ask.notSure.search", { topic })}
                  </a>
                </>
              ) : null}

              {phase === "nothing" ? (
                <>
                  <p className={styles.lead}>{t("ask.nothing.lead", { topic })}</p>
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

              {phase === "done" || forgotten ? (
                <Actions
                  memories={forgotten ? [] : memories}
                  result={result}
                  onResult={setResult}
                  onForgotten={() => setForgotten(true)}
                  resultRef={resultRef}
                />
              ) : null}
            </div>
          </article>
        ) : (
          <EmptyAsk onAsk={start} />
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
    </div>
  );
}

function EmptyAsk({ onAsk }: { onAsk: (question: string) => void }) {
  const EXAMPLES = [t("ask.example.1"), t("ask.example.2"), t("ask.example.3")];
  return (
    <section className={styles.empty}>
      <h1 className={styles.question}>{t("ask.empty.title")}</h1>
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
