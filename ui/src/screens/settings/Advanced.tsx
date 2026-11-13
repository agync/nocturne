import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { goTo } from "../../app/query";
import { Button } from "../../components/Button/Button";
import { formatBytes } from "../../copy/format";
import { t } from "../../copy/t";
import type { AdvancedView, ComponentRole } from "../../engine/bindings";
import { useEngine, useRefresh, useSettings } from "../../engine/context";
import { usePlatform } from "../../platform/context";
import { DeleteEverythingDialog } from "../privacy/DeleteEverythingDialog";
import styles from "./Settings.module.css";

function uptime(seconds: number | null): string {
  if (seconds === null) return t("advanced.engine.off");
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  if (days > 0) return t("advanced.uptime.days", { days, hours });
  if (hours > 0) return t("advanced.uptime.hours", { hours, minutes });
  return t("advanced.uptime.minutes", { minutes });
}

function component(view: AdvancedView, role: ComponentRole) {
  return view.components.find((c) => c.role === role);
}

/** The details, row by row, in the words an engineer would use. The only place for them. */
export function details(view: AdvancedView, voice: string): Array<[string, string]> {
  const mind = component(view, "mind");
  const speech = component(view, "speech");
  const voiceC = component(view, "voice");
  const recall = component(view, "recall");
  const sized = (c: typeof mind) =>
    c
      ? c.backend
        ? t("advanced.component.backend", {
            name: c.name,
            size: formatBytes(c.bytes),
            backend: c.backend,
          })
        : t("advanced.component", { name: c.name, size: formatBytes(c.bytes) })
      : "–";
  return [
    [
      t("advanced.row.tier"),
      view.tier
        ? t("advanced.tier", {
            tier: view.tier,
            ram: view.ramGib,
            gpu: view.gpu ?? t("advanced.gpu.none"),
            npu: t(view.npu ? "advanced.npu.yes" : "advanced.npu.no"),
          })
        : t("advanced.tier.none"),
    ],
    [t("advanced.row.mind"), sized(mind)],
    [t("advanced.row.vision"), t("advanced.vision")],
    [
      t("advanced.row.speech"),
      speech
        ? t("advanced.component", { name: speech.name, size: formatBytes(speech.bytes) })
        : "–",
    ],
    [
      t("advanced.row.voice"),
      voiceC
        ? t("advanced.voice", { name: voiceC.name, size: formatBytes(voiceC.bytes), voice })
        : "–",
    ],
    [
      t("advanced.row.recall"),
      recall
        ? t("advanced.component", { name: recall.name, size: formatBytes(recall.bytes) })
        : "–",
    ],
    [
      t("advanced.row.engine"),
      view.enginePort === null
        ? t("advanced.engine.off")
        : t("advanced.engine", {
            version: view.engineVersion ?? "?",
            port: view.enginePort,
            uptime: uptime(view.engineUptimeSeconds),
            restarts: view.restarts,
          }),
    ],
    [
      t("advanced.row.store"),
      t("advanced.store", {
        version: view.storeVersion,
        memories: view.memoryCount.toLocaleString("en-GB"),
        items: view.itemCount.toLocaleString("en-GB"),
        integrity: t(view.integrityOk ? "advanced.integrity.ok" : "advanced.integrity.bad"),
      }),
    ],
    [
      t("advanced.row.tidy"),
      view.lastTidy
        ? t("advanced.tidy", {
            from: view.lastTidy.from,
            to: view.lastTidy.to,
            items: view.lastTidy.items,
            merged: view.lastTidy.merged,
            letGo: view.lastTidy.letGo,
          })
        : t("advanced.tidy.none"),
    ],
    [t("advanced.row.agents"), t(view.agents ? "advanced.on" : "advanced.off")],
    [t("advanced.row.logs"), view.logDir],
  ];
}

/** #/settings/advanced */
export function Advanced() {
  const engine = useEngine();
  const platform = usePlatform();
  const refresh = useRefresh();
  const { data: settings } = useSettings();
  const query = useQuery({ queryKey: ["advanced"], queryFn: () => engine.getAdvanced() });
  const [resetting, setResetting] = useState(false);

  if (query.isError) {
    return (
      <div className={styles.message} role="alert">
        <p>{t("advanced.error")}</p>
        <Button variant="secondary" onClick={() => void query.refetch()}>
          {t("button.tryAgain")}
        </Button>
      </div>
    );
  }
  if (!query.data) {
    return (
      <p className={styles.message} role="status">
        {t("advanced.loading")}
      </p>
    );
  }
  const view = query.data;

  return (
    <div className={styles.frame}>
      <div className={styles.card}>
        <a className={styles.back} href="#/settings">
          {t("advanced.back")}
        </a>
        <h1 className="t-page-title">{t("advanced.title")}</h1>
        <p className={styles.dek}>{t("advanced.dek")}</p>
        <dl className={styles.details}>
          {details(view, settings?.voice ?? "").map(([label, value]) => (
            <div key={label} className={styles.detail}>
              <dt>{label}</dt>
              <dd>{value}</dd>
            </div>
          ))}
        </dl>
        <div className={styles.buttons}>
          <Button variant="secondary" onClick={() => void platform.openPath(view.logDir)}>
            {t("advanced.logs")}
          </Button>
          <Button variant="secondary" onClick={() => goTo("/setup", { step: "choosing" })}>
            {t("advanced.mind")}
          </Button>
          <Button variant="secondary" onClick={() => goTo("/setup", { step: "welcome" })}>
            {t("advanced.setup")}
          </Button>
          <Button variant="forget" onClick={() => setResetting(true)}>
            {t("advanced.reset")}
          </Button>
        </div>
      </div>
      {resetting ? (
        <DeleteEverythingDialog
          title={t("advanced.reset.title")}
          body={t("advanced.reset.body")}
          confirmLabel={t("advanced.reset.confirm")}
          onClose={() => setResetting(false)}
          onDeleted={() => {
            refresh(
              "status",
              "page",
              "pages",
              "items",
              "memories",
              "entities",
              "needsYou",
              "fading",
              "privacy",
              "advanced",
              "setup",
            );
            setResetting(false);
            goTo("/setup", { step: "welcome" });
          }}
        />
      ) : null}
    </div>
  );
}
