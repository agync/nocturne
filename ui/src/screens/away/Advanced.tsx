import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { Button } from "../../components/Button/Button";
import { useToast } from "../../components/Toast/Toast";
import { formatBytes, formatTime } from "../../copy/format";
import { t } from "../../copy/t";
import type { RemoteAdvanced } from "../../engine/bindings";
import { useEngine } from "../../engine/context";
import { usePlatform } from "../../platform/context";
import styles from "./Away.module.css";
import { Confirm } from "./parts";
import { useBusyAct } from "./useRemote";

/** The rows an engineer would read. The only place on these screens where the technical words live. */
export function advancedRows(view: RemoteAdvanced): Array<[string, string]> {
  const simple = view.connection === "simple";
  const rows: Array<[string, string]> = [
    [
      t("advanced.away.row.connection"),
      t(simple ? "advanced.away.connection.simple" : "advanced.away.connection.mostPrivate"),
    ],
  ];
  if (view.helperName) {
    const version = view.helperVersion ?? "?";
    rows.push([
      t("advanced.away.row.helper"),
      simple
        ? t("advanced.away.helper.simple", {
            name: view.helperName,
            version,
            checked: t(view.helperChecked ? "advanced.away.checked" : "advanced.away.unchecked"),
            uptime: formatUptime(view.helperUptimeSeconds),
            restarts: view.helperRestarts,
          })
        : t("advanced.away.helper.mostPrivate", { name: view.helperName, version }),
    ]);
  }
  if (view.machineName) {
    rows.push([
      t("advanced.away.row.machine"),
      t("advanced.away.machine", { name: view.machineName }),
    ]);
  }
  if (view.address) {
    rows.push([
      t("advanced.away.row.address"),
      view.addressSince
        ? t(simple ? "advanced.away.address.simple" : "advanced.away.address.mostPrivate", {
            address: view.address,
            time: formatTime(view.addressSince),
          })
        : view.address,
    ]);
  }
  if (view.listener) rows.push([t("advanced.away.row.listener"), view.listener]);
  if (view.route) rows.push([t("advanced.away.row.route"), view.route]);
  if (view.lastContact) {
    rows.push([
      t("advanced.away.row.lastContact"),
      t("advanced.away.lastContact", {
        name: view.lastContact.deviceName,
        time: formatTime(view.lastContact.at),
      }),
    ]);
  }
  rows.push([t("advanced.away.row.sealing"), t("advanced.away.sealing")]);
  rows.push([
    t("advanced.away.row.middle"),
    t(simple ? "advanced.away.middle.simple" : "advanced.away.middle.mostPrivate"),
  ]);
  if (view.helperEstimateBytesPerDay !== null) {
    rows.push([
      t("advanced.away.row.traffic"),
      t("advanced.away.traffic", { size: formatBytes(view.helperEstimateBytesPerDay) }),
    ]);
  }
  rows.push([t("advanced.away.row.limits"), t("advanced.away.limits")]);
  rows.push([t("advanced.away.row.audit"), t("advanced.away.audit", { count: view.auditEvents })]);
  return rows;
}

function formatUptime(seconds: number | null): string {
  if (seconds === null) return "–";
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  return hours > 0
    ? t("advanced.away.uptime.hours", { hours, minutes })
    : t("advanced.away.uptime.minutes", { minutes });
}

/** What the channel does not hide, as plain lines for the curious. */
function Hides({ simple }: { simple: boolean }) {
  return (
    <div className={styles.hides}>
      <h3 className={styles.smallHeading}>{t("advanced.away.hides.heading")}</h3>
      <ul>
        <li>{t("advanced.away.hides.middle")}</li>
        <li>
          {t(simple ? "advanced.away.hides.page.simple" : "advanced.away.hides.page.mostPrivate")}
        </li>
        <li>{t("advanced.away.hides.address")}</li>
        <li>{t("advanced.away.hides.allow")}</li>
        <li>{t("advanced.away.hides.key")}</li>
        {simple ? null : <li>{t("advanced.away.hides.account")}</li>}
      </ul>
    </div>
  );
}

export function AdvancedDetails({ id }: { id: string }) {
  const engine = useEngine();
  const platform = usePlatform();
  const toast = useToast();
  const { run, busy } = useBusyAct();
  const [making, setMaking] = useState(false);
  const query = useQuery({ queryKey: ["advanced"], queryFn: () => engine.getAdvanced() });

  if (query.isError) {
    return (
      <section id={id} className={styles.advanced} role="alert">
        <p>{t("advanced.away.error")}</p>
      </section>
    );
  }
  if (!query.data) {
    return (
      <section id={id} className={styles.advanced} role="status">
        <p>{t("advanced.away.loading")}</p>
      </section>
    );
  }
  const remote = query.data.remote;
  const rows = remote ? advancedRows(remote) : [];

  const copy = async () => {
    const text = rows.map(([label, value]) => `${label}: ${value}`).join("\n");
    try {
      await navigator.clipboard.writeText(text);
      toast.show({ message: t("advanced.away.copied") });
    } catch {
      toast.show({ message: t("advanced.away.copyFailed") });
    }
  };

  return (
    <section id={id} className={styles.advanced} aria-labelledby={`${id}-title`}>
      <div className={styles.advancedHead}>
        <h2 id={`${id}-title`} className={styles.smallHeading}>
          {t("advanced.away.title")}
        </h2>
        <span className={styles.muted}>{t("advanced.away.dek")}</span>
      </div>
      {remote ? (
        <>
          <dl className={styles.details}>
            {rows.map(([label, value]) => (
              <div key={label} className={styles.detail}>
                <dt>{label}</dt>
                <dd>{value}</dd>
              </div>
            ))}
          </dl>
          <Hides simple={remote.connection === "simple"} />
        </>
      ) : (
        <p className={styles.muted}>{t("advanced.away.off")}</p>
      )}
      <div className={styles.buttons}>
        <Button variant="secondary" onClick={() => void platform.openLogs()}>
          {t("advanced.away.logs")}
        </Button>
        {remote ? (
          <Button variant="secondary" onClick={() => void copy()}>
            {t("advanced.away.copy")}
          </Button>
        ) : null}
        <Button variant="secondary" onClick={() => setMaking(true)}>
          {t("advanced.away.newKey")}
        </Button>
      </div>
      {making ? (
        <Confirm
          title={t("advanced.away.newKey.title")}
          confirm={t("advanced.away.newKey.confirm")}
          keep={t("advanced.away.newKey.keep")}
          busy={busy}
          onClose={() => setMaking(false)}
          onConfirm={() => {
            void run(() => engine.newComputerKey()).then(() => setMaking(false));
          }}
        >
          <p>{t("advanced.away.newKey.body")}</p>
        </Confirm>
      ) : null}
    </section>
  );
}
