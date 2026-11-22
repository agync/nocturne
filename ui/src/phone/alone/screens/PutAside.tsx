import { useEffect, useRef, useState } from "react";
import { formatBytes, formatLongDay } from "../../../copy/format";
import { t } from "../../../copy/t";
import type { PutAsideMemory } from "../../../engine/bindings";
import { Group, Row } from "../../native/screens/chrome";
import { useHeading } from "../../native/screens/parts";
import { BackBar, Button, Page } from "../../screens/ui";
import { Running } from "./Backup";

type Step =
  | { name: "choose"; failed: boolean }
  | { name: "bringBack" }
  | { name: "bringing" }
  | { name: "delete" }
  | { name: "deleting" };

/** A question with the answer that changes nothing first, as 0013's dialogs ask it. */
function Confirm({
  title,
  body,
  keep,
  go,
  careful,
  busy,
  onKeep,
  onGo,
}: {
  title: string;
  body: string;
  keep: string;
  go: string;
  careful?: boolean;
  busy?: boolean;
  onKeep(): void;
  onGo(): void;
}) {
  const foot = useRef<HTMLElement>(null);
  // 0013: the answer that changes nothing has focus.
  useEffect(() => {
    foot.current?.querySelector("button")?.focus();
  }, []);
  return (
    <Page bar={null}>
      <main className="ph-main" style={{ gap: 16 }}>
        <h1 className="ph-steph">{title}</h1>
        <p className="ph-body ph-ink2">{body}</p>
      </main>
      <footer className="ph-foot" ref={foot}>
        <Button onClick={onKeep}>{keep}</Button>
        {careful ? (
          <button
            type="button"
            className="ph-btn ph-btn--ember"
            aria-disabled={busy ? true : undefined}
            onClick={() => !busy && onGo()}
          >
            {go}
          </button>
        ) : (
          <Button kind="primary" disabled={busy} onClick={onGo}>
            {go}
          </Button>
        )}
      </footer>
    </Page>
  );
}

/**
 * A memory put aside on this phone: 0013's "Bring back what was put aside" and "Delete what was
 * put aside", worded for this phone. One that could not be opened can only be deleted.
 */
export function PutAside({
  memory,
  bringBack,
  remove,
  onBack,
  onDone,
}: {
  memory: PutAsideMemory;
  bringBack(id: string): Promise<void>;
  remove(id: string): Promise<void>;
  onBack(): void;
  /** It came back, or it was deleted: This phone shows what is there now. */
  onDone(): void;
}) {
  const [step, setStep] = useState<Step>({ name: "choose", failed: false });
  const heading = useHeading("putAside");
  const day = formatLongDay(memory.at);
  const counts = memory.counts;

  const run = async (busy: Step, work: () => Promise<void>) => {
    setStep(busy);
    try {
      await work();
      onDone();
    } catch {
      setStep({ name: "choose", failed: true });
    }
  };

  if (step.name === "bringing")
    return <Running title={t("privacy.backup.bringBack.restoring")} progress={null} />;
  if (step.name === "bringBack")
    return (
      <Confirm
        title={t("privacy.backup.bringBack.title")}
        body={t("phone.alone.bringBack.body")}
        keep={t("privacy.backup.bringBack.notNow")}
        go={t("privacy.backup.bringBack.confirm")}
        onKeep={() => setStep({ name: "choose", failed: false })}
        onGo={() => void run({ name: "bringing" }, () => bringBack(memory.id))}
      />
    );
  if (step.name === "delete" || step.name === "deleting")
    return (
      <Confirm
        title={t("privacy.backup.delete.title")}
        body={
          counts
            ? t("phone.alone.putAside.delete.body", {
                things: counts.things,
                entries: counts.journalEntries,
                media: counts.photosAndRecordings,
                day,
              })
            : t("phone.alone.putAside.delete.unread", { day })
        }
        keep={t("privacy.backup.delete.keep")}
        go={t("privacy.backup.delete.confirm")}
        careful
        busy={step.name === "deleting"}
        onKeep={() => setStep({ name: "choose", failed: false })}
        onGo={() => void run({ name: "deleting" }, () => remove(memory.id))}
      />
    );

  return (
    <Page bar={<BackBar title={t("phone.sheet.title")} onBack={onBack} />}>
      <main className="ph-main ph-settings">
        <h1 className="ph-steph" tabIndex={-1} ref={heading}>
          {t("phone.alone.putAside.title", { day })}
        </h1>
        <p className="ph-body ph-ink2">
          {counts ? formatBytes(memory.bytes) : t("phone.alone.putAside.unread")}
        </p>
        <Group>
          {counts ? (
            <Row
              label={t("privacy.backup.putAside.bringBack")}
              onPress={() => setStep({ name: "bringBack" })}
            />
          ) : null}
          <Row
            label={t("privacy.backup.putAside.delete")}
            danger
            onPress={() => setStep({ name: "delete" })}
          />
        </Group>
        {step.failed ? (
          <p className="ph-error" role="alert">
            {t("setup.action.failed")}
          </p>
        ) : null}
      </main>
    </Page>
  );
}
