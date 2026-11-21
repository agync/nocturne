import { Children, useEffect, useRef, useState } from "react";
import { goTo } from "../../app/query";
import { Button } from "../../components/Button/Button";
import { Checkbox } from "../../components/Checkbox/Checkbox";
import { TextInput } from "../../components/TextInput/TextInput";
import { useToast } from "../../components/Toast/Toast";
import { t, tParts } from "../../copy/t";
import type {
  DeviceView,
  PhoneDeletePreview,
  PhoneSource,
  PhoneSources,
  RemoteView,
} from "../../engine/bindings";
import { useEngine } from "../../engine/context";
import { DeleteRows } from "../phones/DeleteRows";
import { bringsKey, phoneKind, usePhoneSources } from "../phones/usePhoneSources";
import styles from "./Away.module.css";
import { Confirm, DeviceStatus, Frame } from "./parts";
import { useRemoteAct } from "./useRemote";
import { whenText } from "./when";

const MAX_NAME_BYTES = 40;

type Previews =
  | { state: "idle" | "loading" | "failed" }
  | { state: "ready"; rows: { source: PhoneSource; preview: PhoneDeletePreview }[] };

function nameProblem(name: string): "away.phones.name.empty" | "away.phones.name.long" | null {
  const clean = name.trim();
  if (clean === "") return "away.phones.name.empty";
  if (new TextEncoder().encode(clean).length > MAX_NAME_BYTES) return "away.phones.name.long";
  return null;
}

interface RowProps {
  device: DeviceView;
  brought?: PhoneSources;
  renaming: boolean;
  onRename: () => void;
  onDone: () => void;
  onRemove: () => void;
}

/** What kind of phone it is and what it brings, when the computer knows. */
function PhoneKind({ phone }: { phone: PhoneSources }) {
  const brings = bringsKey(phone);
  const line = [phoneKind(phone), brings ? t(brings) : null].filter(Boolean).join(" · ");
  return line === "" ? null : <span className={styles.sub}>{line}</span>;
}

function PhoneRow({ device, brought, renaming, onRename, onDone, onRemove }: RowProps) {
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
            {brought ? <PhoneKind phone={brought} /> : null}
          </>
        )}
      </div>
      <span className={styles.paired}>
        {t("away.phones.paired", { when: whenText(device.pairedAt) })}
      </span>
      {renaming ? null : (
        <span className={styles.buttonsInline} ref={actions}>
          {brought && bringsKey(brought) ? (
            <a className="hit" href="#/settings/phones">
              {t("screen.phones.title")}
            </a>
          ) : null}
          <Button
            variant="quiet"
            aria-label={t("away.phones.rename.named", { name: device.name })}
            onClick={onRename}
          >
            {t("away.phones.rename")}
          </Button>
          <Button
            variant="forget"
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
  const [alsoDelete, setAlsoDelete] = useState(false);
  const { phones, reread } = usePhoneSources();
  const brought = phones ?? [];
  const bringing = removing ? brought.find((p) => p.deviceId === removing.id) : undefined;
  const brings = bringing ? bringsKey(bringing) !== null : false;

  const listing = alsoDelete && bringing !== undefined && brings;
  const removingId = removing?.id;
  const sourcesBrought = (["calendar", "health"] as const).filter((s) => bringing?.[s] != null);
  const sourcesKey = sourcesBrought.join(",");
  const [previews, setPreviews] = useState<Previews>({ state: "idle" });
  const latest = useRef(0);
  useEffect(() => {
    const request = ++latest.current;
    if (!listing || !removingId || sourcesKey === "") {
      setPreviews({ state: "idle" });
      return;
    }
    setPreviews({ state: "loading" });
    const sources = sourcesKey.split(",") as PhoneSource[];
    Promise.all(
      sources.map((source) =>
        engine.previewPhoneDelete({ deviceId: removingId, source, kinds: null, before: null }),
      ),
    ).then(
      (answers) => {
        if (request !== latest.current) return;
        setPreviews({
          state: "ready",
          rows: sources.map((source, i) => ({ source, preview: answers[i] as PhoneDeletePreview })),
        });
      },
      () => {
        if (request === latest.current) setPreviews({ state: "failed" });
      },
    );
  }, [engine, listing, removingId, sourcesKey]);
  const waiting = alsoDelete && brings && previews.state !== "ready";

  const remove = async () => {
    if (!removing) return;
    setBusy(true);
    const name = removing.name;
    const withBrought = brings && alsoDelete;
    const done = await act(() =>
      withBrought ? engine.removeDevice(removing.id, true) : engine.removeDevice(removing.id),
    );
    if (done) {
      void reread();
      toast.show({ message: t("away.phones.removed", { name }) });
    }
    setBusy(false);
    setRemoving(null);
  };

  return (
    <Frame
      back={{ to: "/settings/away", label: t("screen.away.title") }}
      aside={
        <>
          <h2 className={styles.label}>{t("away.phones.lost.heading")}</h2>
          <p className={styles.muted}>{t("away.phones.lost.body")}</p>
          <p className={styles.muted}>{t("away.phones.holds")}</p>
        </>
      }
    >
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
              brought={brought.find((p) => p.deviceId === device.id)}
              renaming={renaming === device.id}
              onRename={() => setRenaming(device.id)}
              onDone={() => setRenaming(null)}
              onRemove={() => {
                setAlsoDelete(false);
                setRemoving(device);
              }}
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
        <a className="hit" href="#/settings/away/activity">
          {t("away.link.activity")}
        </a>
      </div>
      {removing ? (
        <Confirm
          title={t("away.remove.title", { name: removing.name })}
          confirm={t("away.remove.confirm")}
          keep={t("away.remove.keep")}
          destructive
          busy={busy}
          blocked={waiting}
          onClose={() => setRemoving(null)}
          onConfirm={() => void remove()}
        >
          <p>{t("away.remove.body")}</p>
          <p>{t("away.remove.kept")}</p>
          {brings ? (
            <>
              <p>
                {Children.toArray(
                  tParts("phones.remove.adds", {
                    fromYourPhones: <a href="#/settings/phones">{t("screen.phones.title")}</a>,
                  }) as React.ReactNode[],
                )}
              </p>
              <Checkbox
                checked={alsoDelete}
                onChange={setAlsoDelete}
                disabled={busy}
                label={t("phones.remove.alsoDelete")}
              />
              {alsoDelete && previews.state === "loading" ? (
                <p role="status">{t("phones.delete.loading")}</p>
              ) : null}
              {alsoDelete && previews.state === "failed" ? (
                <p role="alert">{t("phones.delete.error")}</p>
              ) : null}
              {previews.state === "ready" && alsoDelete
                ? previews.rows.map(({ source, preview }) => (
                    <DeleteRows
                      key={source}
                      source={source}
                      preview={preview}
                      phoneName={removing.name}
                      kinds={bringing?.health?.kinds ?? []}
                    />
                  ))
                : null}
            </>
          ) : null}
        </Confirm>
      ) : null}
    </Frame>
  );
}
