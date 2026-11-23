import { useEffect, useState } from "react";
import { formatBytes, formatLongDay, formatTimeOfDay, journalDay } from "../../../copy/format";
import { type CopyId, t } from "../../../copy/t";
import { useHeading } from "../../native/screens/parts";
import { BackBar, Page } from "../../screens/ui";
import type { MindSent } from "../mind";

const KIND: Record<MindSent["kind"], CopyId> = {
  read: "phone.mind.sent.read",
  answer: "phone.mind.sent.answer",
  page: "phone.mind.sent.page",
  check: "phone.mind.sent.check",
};

type Load = { name: "loading" } | { name: "ready"; rows: MindSent[] } | { name: "failed" };

/** One request: what it was for, when it left and how much. */
function SentRow({ row }: { row: MindSent }) {
  return (
    <li className="ph-mind-sent">
      <span className="ph-mind-sent-what">{t(KIND[row.kind])}</span>
      <span className="ph-mind-sent-meta">
        {t("phone.mind.sent.row", {
          time: formatTimeOfDay(row.at),
          size: formatBytes(row.bytes),
        })}
      </span>
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
                {rows.map((row) => (
                  <SentRow key={row.id} row={row} />
                ))}
              </ul>
            </section>
          ))
        )}
        {load.name === "ready" ? <p className="ph-small">{t("phone.mind.sent.footer")}</p> : null}
      </main>
    </Page>
  );
}
