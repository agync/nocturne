import { useEffect, useRef, useState } from "react";
import { goTo } from "../../app/query";
import { Button } from "../../components/Button/Button";
import { TextInput } from "../../components/TextInput/TextInput";
import { useToast } from "../../components/Toast/Toast";
import { t } from "../../copy/t";
import type { DeviceView, RemoteView } from "../../engine/bindings";
import { useEngine } from "../../engine/context";
import styles from "./Away.module.css";
import { Back, Confirm, DeviceStatus, Frame } from "./parts";
import { useRemoteAct } from "./useRemote";
import { whenText } from "./when";

const MAX_NAME_BYTES = 40;

function nameProblem(name: string): "away.phones.name.empty" | "away.phones.name.long" | null {
  const clean = name.trim();
  if (clean === "") return "away.phones.name.empty";
  if (new TextEncoder().encode(clean).length > MAX_NAME_BYTES) return "away.phones.name.long";
  return null;
}

interface RowProps {
  device: DeviceView;
  renaming: boolean;
  onRename: () => void;
  onDone: () => void;
  onRemove: () => void;
}

function PhoneRow({ device, renaming, onRename, onDone, onRemove }: RowProps) {
  const engine = useEngine();
  const act = useRemoteAct();
  const [draft, setDraft] = useState(device.name);
  const [error, setError] = useState<string | undefined>();
  const actions = useRef<HTMLSpanElement>(null);
  const wasRenaming = useRef(false);

  useEffect(() => {
    // Back on the button that opened the field, so the keyboard does not lose its place.
    if (wasRenaming.current && !renaming) actions.current?.querySelector("button")?.focus();
    wasRenaming.current = renaming;
    if (renaming) {
      setDraft(device.name);
      setError(undefined);
    }
  }, [renaming, device.name]);

  const save = async () => {
    const problem = nameProblem(draft);
    if (problem) {
      setError(t(problem));
      return;
    }
    if (draft.trim() === device.name) {
      onDone();
      return;
    }
    if (await act(() => engine.renameDevice(device.id, draft.trim()))) onDone();
  };

  return (
    <li className={styles.phone}>
      <div className={styles.phoneText}>
        {renaming ? (
          <form
            className={styles.renameForm}
            onSubmit={(event) => {
              event.preventDefault();
              void save();
            }}
            onKeyDown={(event) => {
              if (event.key === "Escape") {
                event.stopPropagation();
                onDone();
              }
            }}
          >
            <TextInput
              label={t("away.phones.renameField", { name: device.name })}
              value={draft}
              onChange={(value) => {
                setDraft(value);
                setError(undefined);
              }}
              error={error}
              autoFocus
            />
            <div className={styles.buttonsInline}>
              <Button variant="primary" type="submit">
                {t("away.phones.save")}
              </Button>
              <Button variant="secondary" onClick={onDone}>
                {t("away.cancel")}
              </Button>
            </div>
          </form>
        ) : (
          <>
            <span className={styles.name}>{device.name}</span>
            <span className={styles.sub}>
              <DeviceStatus device={device} />
            </span>
          </>
        )}
      </div>
      <span className={styles.paired}>
        {t("away.phones.paired", { when: whenText(device.pairedAt) })}
      </span>
      {renaming ? null : (
        <span className={styles.buttonsInline} ref={actions}>
          <Button
            variant="quiet"
            aria-label={t("away.phones.rename.named", { name: device.name })}
            onClick={onRename}
          >
            {t("away.phones.rename")}
          </Button>
          <Button
            variant="quiet"
            aria-label={t("away.phones.remove.named", { name: device.name })}
            onClick={onRemove}
          >
            {t("away.phones.remove")}
          </Button>
        </span>
      )}
    </li>
  );
}

/** #/settings/away/phones */
export function Phones({ view }: { view: RemoteView }) {
  const engine = useEngine();
  const act = useRemoteAct();
  const toast = useToast();
  const [renaming, setRenaming] = useState<string | null>(null);
  const [removing, setRemoving] = useState<DeviceView | null>(null);
  const [busy, setBusy] = useState(false);

  const remove = async () => {
    if (!removing) return;
    setBusy(true);
    const name = removing.name;
    if (await act(() => engine.removeDevice(removing.id))) {
      toast.show({ message: t("away.phones.removed", { name }) });
    }
    setBusy(false);
    setRemoving(null);
  };

  return (
    <Frame
      aside={
        <>
          <h2 className={styles.label}>{t("away.phones.lost.heading")}</h2>
          <p className={styles.muted}>{t("away.phones.lost.body")}</p>
          <p className={styles.muted}>{t("away.phones.holds")}</p>
        </>
      }
    >
      <Back to="/settings/away" label={t("away.back.away")} />
      <h1 className="t-page-title">{t("away.phones.title")}</h1>
      <p className={styles.lede}>{t("away.phones.lead")}</p>
      {view.devices.length === 0 ? (
        <p className={styles.body}>{t("away.phones.empty")}</p>
      ) : (
        <ul className={styles.phones} aria-label={t("away.phones.list")}>
          {view.devices.map((device) => (
            <PhoneRow
              key={device.id}
              device={device}
              renaming={renaming === device.id}
              onRename={() => setRenaming(device.id)}
              onDone={() => setRenaming(null)}
              onRemove={() => setRemoving(device)}
            />
          ))}
        </ul>
      )}
      <div className={styles.actions}>
        <Button
          variant="primary"
          disabled={view.state.type !== "on"}
          onClick={() => goTo("/settings/away/add")}
        >
          {t("away.phones.add")}
        </Button>
        <a href="#/settings/away/activity">{t("away.link.activity")}</a>
      </div>
      {removing ? (
        <Confirm
          title={t("away.remove.title", { name: removing.name })}
          confirm={t("away.remove.confirm")}
          keep={t("away.remove.keep")}
          busy={busy}
          onClose={() => setRemoving(null)}
          onConfirm={() => void remove()}
        >
          <p>{t("away.remove.body")}</p>
          <p>{t("away.remove.kept")}</p>
        </Confirm>
      ) : null}
    </Frame>
  );
}
