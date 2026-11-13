import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { goTo } from "../../app/query";
import { recordingState } from "../../app/useRecorder";
import { BackLink } from "../../components/BackLink/BackLink";
import { Button } from "../../components/Button/Button";
import { ScreenSheet } from "../../components/ScreenSheet/ScreenSheet";
import { formatBytes, formatTime } from "../../copy/format";
import { type CopyId, t } from "../../copy/t";
import type { AdvancedView, ComponentRole, OnlineAdvanced } from "../../engine/bindings";
import type { RecordingState } from "../../engine/client";
import { useEngine, useRefresh, useSettings } from "../../engine/context";
import { usePlatform } from "../../platform/context";
import { useModels } from "../models/useModels";
import { DeleteEverythingDialog } from "../privacy/DeleteEverythingDialog";
import styles from "./Settings.module.css";

const RECORDING_COPY: Record<RecordingState, CopyId> = {
  available: "advanced.recording.available",
  "not-secure": "advanced.recording.not-secure",
  "no-devices": "advanced.recording.no-devices",
  "no-recorder": "advanced.recording.no-recorder",
};

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
export function details(
  view: AdvancedView,
  voice: string,
  recording: RecordingState = recordingState(),
): Array<[string, string]> {
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
    [t("advanced.row.recording"), t(RECORDING_COPY[recording])],
    [t("advanced.row.logs"), view.logDir],
  ];
}

/** Online answers as the engine holds it. Never the key, only where it is kept. */
function onlineDetails(o: OnlineAdvanced): Array<[string, string]> {
  const last =
    o.lastAt !== null &&
    o.lastStatus !== null &&
    o.lastSeconds !== null &&
    o.lastBytesUp !== null &&
    o.lastBytesDown !== null
      ? t("advanced.online.last", {
          time: formatTime(o.lastAt),
          status: o.lastStatus,
          seconds: o.lastSeconds.toFixed(1),
          up: formatBytes(o.lastBytesUp),
          down: formatBytes(o.lastBytesDown),
        })
      : t("advanced.online.last.none");
  const key =
    o.keyStorage === "keychain"
      ? "advanced.online.key.keychain"
      : o.keyStorage === "file"
        ? "advanced.online.key.file"
        : "advanced.online.key.none";
  return [
    [
      t("advanced.online.row.state"),
      t("advanced.online.state", { state: o.state, count: o.questionsThisMonth }),
    ],
    [
      t("advanced.online.row.sentBy"),
      t("advanced.online.sentBy", { route: o.route, url: o.baseUrl ?? "" }),
    ],
    [
      t("advanced.online.row.providers"),
      o.host
        ? t("advanced.online.providers", { count: 1, host: o.host })
        : t("advanced.online.providers.none"),
    ],
    [
      t("advanced.online.row.model"),
      t("advanced.online.model", { name: o.model ?? "", max: o.maxTokens }),
    ],
    [t("advanced.online.row.key"), t(key)],
    [t("advanced.online.row.retention"), t("advanced.online.retention")],
    [t("advanced.online.row.last"), last],
    [
      t("advanced.online.row.cost"),
      t(o.costReported ? "advanced.online.cost.reported" : "advanced.online.cost.unreported"),
    ],
    [t("advanced.online.row.log"), t("advanced.online.log", { count: o.sendsKept })],
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
  const models = useModels().data;

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
    <ScreenSheet>
      <BackLink to="/settings" label={t("screen.settings.title")} />
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
      {view.online ? (
        <section aria-labelledby="advanced-online">
          <h2 id="advanced-online" className="t-section">
            {t("advanced.online.heading")}
          </h2>
          <dl className={styles.details}>
            {onlineDetails(view.online).map(([label, value]) => (
              <div key={label} className={styles.detail}>
                <dt>{label}</dt>
                <dd>{value}</dd>
              </div>
            ))}
          </dl>
        </section>
      ) : null}
      {models ? (
        <a className={`${styles.advanced} hit`} href="#/settings/models">
          {t("advanced.models.link")}
        </a>
      ) : null}
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
      {resetting ? (
        <DeleteEverythingDialog
          title={t("advanced.reset.title")}
          body={t("advanced.reset.body")}
          confirmLabel={t("advanced.reset.confirm")}
          online={view.online?.keyStorage ? view.online.host : null}
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
    </ScreenSheet>
  );
}
