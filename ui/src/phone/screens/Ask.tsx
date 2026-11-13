import { type FormEvent, useCallback, useEffect, useRef, useState } from "react";
import { richText } from "../../components/PageLine/richText";
import { t } from "../../copy/t";
import type { AskDelta, AskPhase, NearMatch, Receipt } from "../../engine/bindings";
import { ConnectionError, type PhoneEngine, PhoneRequestError } from "../engine";
import { receiptMeta } from "../labels";
import { MAX_QUESTION } from "../protocol";
import { BackBar, Icon, Page } from "./ui";

export type AskFailure = "tooMany" | "notReady" | "paused" | "failed";

export interface AskModel {
  question: string | null;
  phase: AskPhase | "starting" | "stopped";
  text: string;
  receipts: Receipt[];
  near: NearMatch[];
  failure: AskFailure | null;
}

const EMPTY: AskModel = {
  question: null,
  phase: "done",
  text: "",
  receipts: [],
  near: [],
  failure: null,
};

const encoder = new TextEncoder();

/** The question cut to what a phone may send, at a character boundary. */
export function capQuestion(text: string): string {
  if (encoder.encode(text).length <= MAX_QUESTION) return text;
  let out = "";
  let size = 0;
  for (const ch of text) {
    const n = encoder.encode(ch).length;
    if (size + n > MAX_QUESTION) break;
    out += ch;
    size += n;
  }
  return out;
}

/** The conversation lives above the screen, so opening an original and coming back finds it. */
export function useAskSession(engine: PhoneEngine) {
  const [model, setModel] = useState<AskModel>(EMPTY);
  const current = useRef<{ id: number; askId: string | null; stopped: boolean }>({
    id: 0,
    askId: null,
    stopped: false,
  });

  const ask = useCallback(
    async (question: string) => {
      const run = { id: current.current.id + 1, askId: null as string | null, stopped: false };
      current.current = run;
      setModel({ question, phase: "starting", text: "", receipts: [], near: [], failure: null });
      const onDelta = (d: AskDelta) => {
        if (current.current.id !== run.id || run.stopped) return;
        run.askId = d.askId;
        setModel((m) => ({
          ...m,
          phase: d.phase,
          text: m.text + (d.textDelta ?? ""),
          receipts: d.receipts.length > 0 ? d.receipts : m.receipts,
          near: d.near.length > 0 ? d.near : m.near,
        }));
      };
      try {
        await engine.ask(question, onDelta);
      } catch (e) {
        if (current.current.id !== run.id || run.stopped) return;
        let failure: AskFailure = "failed";
        if (e instanceof PhoneRequestError) {
          if (e.code === "tooMany" || e.code === "busy") failure = "tooMany";
          else if (e.code === "notReady") failure = "notReady";
          else if (e.code === "paused") failure = "paused";
        } else if (!(e instanceof ConnectionError)) {
          throw e;
        }
        setModel((m) => ({ ...m, phase: "unavailable", failure }));
      }
    },
    [engine],
  );

  const stop = useCallback(async () => {
    const run = current.current;
    run.stopped = true;
    setModel((m) => ({ ...m, phase: "stopped" }));
    if (run.askId) await engine.cancelAsk(run.askId).catch(() => undefined);
  }, [engine]);

  return { model, ask, stop };
}

const UNIQUE = (receipts: Receipt[]): Receipt[] => {
  const seen = new Set<string>();
  const out: Receipt[] = [];
  for (const r of receipts) {
    if (seen.has(r.itemId)) continue;
    seen.add(r.itemId);
    out.push(r);
  }
  return out;
};

function ReceiptCard({
  receipt,
  onOpen,
  title,
}: {
  receipt: Receipt;
  onOpen(r: Receipt): void;
  title?: string;
}) {
  return (
    <li>
      <button type="button" className="ph-rcpt" onClick={() => onOpen(receipt)}>
        <span className="ph-rcpt-text">
          <span className={`ph-rcpt-title${title ? " ph-rcpt-title--serif" : ""}`}>
            {title ?? (receipt.label || receiptMeta(receipt))}
          </span>
          <span className="ph-rc">{receiptMeta(receipt)}</span>
        </span>
        <Icon name="forward" size={18} />
      </button>
    </li>
  );
}

interface AskProps {
  model: AskModel;
  connected: boolean;
  onAsk(question: string): void;
  onStop(): void;
  onOpen(receipt: Receipt): void;
  onBack(): void;
}

export function Ask({ model, connected, onAsk, onStop, onOpen, onBack }: AskProps) {
  const [draft, setDraft] = useState("");
  const working =
    model.phase === "starting" ||
    ["waking", "searching", "reading", "streaming"].includes(model.phase);
  const answerRef = useRef<HTMLHeadingElement>(null);
  const receipts = UNIQUE(model.receipts);

  useEffect(() => {
    if (model.question && !working) answerRef.current?.focus({ preventScroll: true });
  }, [model.question, working]);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    const q = draft.trim();
    if (!q || working || !connected) return;
    setDraft("");
    onAsk(q);
  };

  const done = model.phase === "done" && model.failure === null;
  const note =
    done && receipts.length > 0 ? t("phone.ask.from", { count: receipts.length }) : undefined;

  return (
    <Page bar={<BackBar title={t("phone.ask.title")} onBack={onBack} note={note} />} fixed>
      <main
        className="ph-main ph-main--tight"
        style={{ flex: 1, overflowY: "auto", padding: "22px 20px" }}
      >
        {model.question === null ? (
          <>
            <h2 className="ph-h1 ph-h1--small" style={{ marginTop: 8 }}>
              {t("phone.ask.empty.title")}
            </h2>
            <p className="ph-body">{t("phone.ask.empty.body")}</p>
            <ul className="ph-examples">
              {(["phone.ask.example.1", "phone.ask.example.2", "phone.ask.example.3"] as const).map(
                (id) => (
                  <li key={id}>
                    <button
                      type="button"
                      className="ph-example"
                      disabled={!connected}
                      onClick={() => onAsk(t(id))}
                    >
                      {t(id)}
                    </button>
                  </li>
                ),
              )}
            </ul>
          </>
        ) : (
          <>
            <span className="ph-lbl">{t("phone.ask.eyebrow")}</span>
            <h2 className="ph-question" ref={answerRef} tabIndex={-1}>
              {t("phone.ask.question", { question: model.question })}
            </h2>

            {model.failure ? (
              <p
                className="ph-error"
                role="alert"
                style={{ color: "var(--ink)", fontSize: "1rem" }}
              >
                {model.failure === "tooMany"
                  ? t("phone.ask.tooMany")
                  : model.failure === "notReady"
                    ? t("phone.ask.notReady")
                    : model.failure === "paused"
                      ? t("phone.ask.paused")
                      : t("phone.ask.failed")}
              </p>
            ) : model.phase === "unavailable" ? (
              <p className="ph-answer" role="status">
                {t("phone.ask.notReady")}
              </p>
            ) : model.phase === "nothing" ? (
              <>
                <p className="ph-answer" role="status">
                  {t("phone.ask.nothing.lead")}
                </p>
                <p className="ph-body">{t("phone.ask.nothing.body")}</p>
              </>
            ) : model.phase === "notSure" ? (
              <>
                <p className="ph-answer" role="status">
                  {t("phone.ask.notSure.lead", { count: model.near.length })}
                </p>
                <ul className="ph-list">
                  {model.near.map((n) => (
                    <ReceiptCard
                      key={`${n.receipt.itemId}-${n.text}`}
                      receipt={n.receipt}
                      title={n.text}
                      onOpen={onOpen}
                    />
                  ))}
                </ul>
                <p className="ph-body" style={{ fontSize: "0.9375rem" }}>
                  {t("phone.ask.notSure.hint")}
                </p>
              </>
            ) : (
              <>
                <p className="ph-answer" aria-live="polite">
                  {model.text === "" && working ? (
                    <span className="ph-rc">
                      {model.phase === "waking" ? t("phone.ask.waking") : ""}
                    </span>
                  ) : (
                    richText(model.text)
                  )}
                  {working && <span className="ph-caret" aria-hidden="true" />}
                </p>
                {working && (
                  <div className="ph-row-between" role="status" style={{ marginTop: 6 }}>
                    <span className="ph-status" style={{ padding: 0 }}>
                      <span className="ph-dot ph-dot--breathe" aria-hidden="true" />
                      {t("phone.ask.working")}
                    </span>
                    <button type="button" className="ph-ib ph-ib--line" onClick={onStop}>
                      {t("phone.ask.stop")}
                    </button>
                  </div>
                )}
                {model.phase === "stopped" && (
                  <p className="ph-hint" role="status">
                    {t("phone.ask.stopped")}
                  </p>
                )}
                {done && receipts.length > 0 && (
                  <ul
                    className="ph-list"
                    aria-label={t("phone.ask.from", { count: receipts.length })}
                  >
                    {receipts.map((r) => (
                      <ReceiptCard key={r.itemId} receipt={r} onOpen={onOpen} />
                    ))}
                  </ul>
                )}
              </>
            )}
            <div className="ph-spacer" />
            {working && (
              <p className="ph-rc" style={{ color: "var(--ink2)" }}>
                {t("phone.ask.cameLater")}
              </p>
            )}
            {done && <p className="ph-hint ph-hint--ink2">{t("phone.ask.wrong")}</p>}
          </>
        )}
      </main>
      <form className="ph-askbar" aria-label={t("phone.ask.title")} onSubmit={submit}>
        <div className="ph-askbox">
          <input
            aria-label={t("phone.ask.field")}
            placeholder={
              model.question === null
                ? t("phone.ask.placeholder.first")
                : t("phone.ask.placeholder")
            }
            value={draft}
            disabled={!connected}
            autoComplete="off"
            enterKeyHint="send"
            onChange={(e) => setDraft(capQuestion(e.target.value))}
          />
          <button
            type="submit"
            className="ph-ib ph-ib--ink"
            style={{ fontWeight: 500 }}
            aria-disabled={working || !connected || draft.trim() === "" ? true : undefined}
          >
            {t("phone.ask.send")}
          </button>
        </div>
      </form>
    </Page>
  );
}
