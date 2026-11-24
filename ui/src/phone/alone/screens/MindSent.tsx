import { useEffect, useState } from "react";
import { formatBytes, formatLongDay, formatTimeOfDay, journalDay } from "../../../copy/format";
import { type CopyId, t } from "../../../copy/t";
import type { ReadingKind } from "../../../engine/bindings";
import { useHeading } from "../../native/screens/parts";
import { BackBar, Page } from "../../screens/ui";
import type { MindAsked, MindSaid, MindSent } from "../mind";

const KIND: Record<MindSent["kind"], CopyId> = {
  read: "phone.mind.sent.read",
  answer: "phone.mind.sent.answer",
  page: "phone.mind.sent.page",
  check: "phone.mind.sent.check",
};

type Load = { name: "loading" } | { name: "ready"; rows: MindSent[] } | { name: "failed" };

/** What one part of a request carried, as it left. */
function Said({ part }: { part: MindSaid }) {
  if (part.kind === "photo") return <p className="ph-small">{t("phone.mind.sent.photo")}</p>;
  if (part.kind === "recording")
    return <p className="ph-small">{t("phone.mind.sent.recording")}</p>;
  return <pre className="ph-mind-sent-words">{part.text}</pre>;
}

/** A kind of reading as "Your provider asked to look through your {kind}" names it. */
const KIND_WORDS: Record<ReadingKind, CopyId> = {
  sleep: "phone.mind.sent.kind.sleep",
  steps: "phone.mind.sent.kind.steps",
  restingHeartRate: "phone.mind.sent.kind.restingHeartRate",
  heartRate: "phone.mind.sent.kind.heartRate",
  bloodPressure: "phone.mind.sent.kind.bloodPressure",
  workout: "phone.mind.sent.kind.workout",
  weight: "phone.mind.sent.kind.weight",
};

/** "Wednesday 25 November", or "Saturday 28 to Sunday 29 November". */
export function spokenDays(from: string, to: string): string {
  const first = formatLongDay(from);
  if (from === to) return first;
  const last = formatLongDay(to);
  const month = (s: string) => s.split(" ").slice(2).join(" ");
  const short = month(first) === month(last) ? first.split(" ").slice(0, 2).join(" ") : first;
  return `${short} to ${last}`;
}

/** What the provider asked to look up, in plain words. */
function askedLine(a: MindAsked): string {
  const days = spokenDays(a.from ?? "", a.to ?? a.from ?? "");
  switch (a.look) {
    case "calendar":
      return t("phone.mind.sent.asked.calendar", { days });
    case "journal":
      return t("phone.mind.sent.asked.journal", { days });
    case "search":
      return t("phone.mind.sent.asked.search", { words: a.words ?? "" });
    case "health":
      return t("phone.mind.sent.asked.health", {
        kind: a.kind ? t(KIND_WORDS[a.kind]) : "",
        days,
      });
  }
}

/** One part of a question: its size, what the provider asked for, and what went back. */
function Part({ row }: { row: MindSent }) {
  const part = row.part;
  if (!part) return null;
  const head = (
    <p className="ph-mind-part-h">
      {t("phone.mind.sent.part", { n: part.n, size: formatBytes(row.bytes) })}
    </p>
  );
  if (part.n === 1)
    return (
      <div className="ph-mind-part">
        {head}
        {row.said.map((said, i) => (
          <Said key={String(i)} part={said} />
        ))}
      </div>
    );
  const back = row.said.slice(part.from);
  const nothing = part.asked.every((a) => a.lines === 0);
  return (
    <div className="ph-mind-part">
      {head}
      {part.asked.map((a, i) => (
        <p key={String(i)} className="ph-small">
          {askedLine(a)}
        </p>
      ))}
      {nothing ? (
        <p className="ph-small">{t("phone.mind.sent.backNothing")}</p>
      ) : (
        <>
          <p className="ph-small">{t("phone.mind.sent.back")}</p>
          {back.map((said, i) => (
            <Said key={String(i)} part={said} />
          ))}
        </>
      )}
    </div>
  );
}

/** A question sent in parts, as one entry: every part's size added up, and each part. */
function QuestionRow({ first, parts }: { first: MindSent; parts: MindSent[] }) {
  const ordered = [first, ...parts].sort((a, b) => (a.part?.n ?? 0) - (b.part?.n ?? 0));
  const size = formatBytes(ordered.reduce((sum, p) => sum + p.bytes, 0));
  const time = formatTimeOfDay(ordered[0]?.at ?? first.at);
  const forgotten = ordered.some((p) => p.forgotten);
  return (
    <li>
      <details className="ph-mind-sent">
        <summary>
          <span className="ph-mind-sent-what">{t("phone.mind.sent.answer")}</span>
          <span className="ph-mind-sent-meta">
            {t("phone.mind.sent.parts", {
              time,
              host: first.host,
              size,
              count: ordered.length,
            })}
          </span>
          {ordered.some((p) => p.nothing) ? (
            <span className="ph-mind-sent-meta">{t("phone.mind.sent.nothing")}</span>
          ) : null}
          {forgotten ? (
            <span className="ph-mind-sent-meta">{t("phone.mind.sent.forgotten")}</span>
          ) : null}
        </summary>
        <p className="ph-lbl">{t("phone.mind.sent.exact")}</p>
        {ordered.map((p) => (
          <Part key={p.id} row={p} />
        ))}
        <p className="ph-small">{t("phone.mind.sent.resent")}</p>
      </details>
    </li>
  );
}

/** One entry of the list: a request, and the other parts of its question, if any. */
export interface Entry {
  first: MindSent;
  rest: MindSent[];
}

/** The list's entries: a request, or every part of one question together where it is newest. */
export function entries(rows: MindSent[]): Entry[] {
  const out: Entry[] = [];
  const byAsk = new Map<string, Entry>();
  for (const row of rows) {
    const ask = row.part?.ask;
    const held = ask ? byAsk.get(ask) : undefined;
    if (held) {
      held.rest.push(row);
      continue;
    }
    const entry = { first: row, rest: [] };
    if (ask) byAsk.set(ask, entry);
    out.push(entry);
  }
  return out;
}

/** One request: what it was for, when, to which host and how much, its words behind a tap. */
function SentRow({ row }: { row: MindSent }) {
  const head = (
    <>
      <span className="ph-mind-sent-what">{t(KIND[row.kind])}</span>
      <span className="ph-mind-sent-meta">
        {t("phone.mind.sent.row", {
          time: formatTimeOfDay(row.at),
          host: row.host,
          size: formatBytes(row.bytes),
        })}
      </span>
      {row.nothing ? (
        <span className="ph-mind-sent-meta">{t("phone.mind.sent.nothing")}</span>
      ) : null}
      {row.forgotten ? (
        <span className="ph-mind-sent-meta">{t("phone.mind.sent.forgotten")}</span>
      ) : null}
    </>
  );
  return (
    <li>
      {row.said.length > 0 ? (
        <details className="ph-mind-sent">
          <summary>{head}</summary>
          <p className="ph-lbl">{t("phone.mind.sent.exact")}</p>
          {row.said.map((part, i) => (
            <Said key={String(i)} part={part} />
          ))}
        </details>
      ) : (
        <div className="ph-mind-sent">{head}</div>
      )}
    </li>
  );
}

/** Sent online on the phone: every request to the chosen service, by day, for 90 days. */
export function MindSentList({
  host,
  sent,
  onBack,
}: {
  host: string;
  sent(): Promise<MindSent[]>;
  onBack(): void;
}) {
  const [load, setLoad] = useState<Load>({ name: "loading" });
  const heading = useHeading("sent");
  useEffect(() => {
    let gone = false;
    sent().then(
      (rows) => !gone && setLoad({ name: "ready", rows }),
      () => !gone && setLoad({ name: "failed" }),
    );
    return () => {
      gone = true;
    };
  }, [sent]);

  const days: [string, MindSent[]][] = [];
  if (load.name === "ready")
    for (const row of load.rows) {
      const day = journalDay(row.at);
      const last = days.at(-1);
      if (last && last[0] === day) last[1].push(row);
      else days.push([day, [row]]);
    }

  return (
    <Page bar={<BackBar title={t("phone.mind.sent.title")} onBack={onBack} />}>
      <main
        className="ph-main ph-main--tight"
        style={{ gap: 14 }}
        aria-busy={load.name === "loading"}
      >
        <h2 className="sr-only" tabIndex={-1} ref={heading}>
          {t("phone.mind.sent.title")}
        </h2>
        <p className="ph-body">{t("phone.mind.sent.dek", { host })}</p>
        {load.name === "loading" ? (
          <p className="ph-small" role="status">
            {t("phone.mind.sent.loading")}
          </p>
        ) : load.name === "failed" ? (
          <p className="ph-error" role="alert">
            {t("phone.mind.sent.error")}
          </p>
        ) : days.length === 0 ? (
          <p className="ph-body">{t("phone.mind.sent.empty")}</p>
        ) : (
          days.map(([day, rows]) => (
            <section
              key={day}
              className="ph-stack"
              style={{ gap: 4 }}
              aria-label={formatLongDay(day)}
            >
              <h3 className="ph-lbl">{formatLongDay(day)}</h3>
              <ul className="ph-mind-sent-list">
                {entries(rows).map(({ first, rest }) =>
                  rest.length > 0 ? (
                    <QuestionRow key={first.id} first={first} parts={rest} />
                  ) : (
                    <SentRow key={first.id} row={first} />
                  ),
                )}
              </ul>
            </section>
          ))
        )}
        {load.name === "ready" ? <p className="ph-small">{t("phone.mind.sent.footer")}</p> : null}
      </main>
    </Page>
  );
}
