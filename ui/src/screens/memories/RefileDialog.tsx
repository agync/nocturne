import { useRef, useState } from "react";
import { Button } from "../../components/Button/Button";
import { Dialog } from "../../components/Dialog/Dialog";
import { TextInput } from "../../components/TextInput/TextInput";
import { t } from "../../copy/t";
import type { EntityKind, MemoryView, Refiled } from "../../engine/bindings";
import { useEngine } from "../../engine/context";
import styles from "./Dialogs.module.css";
import { useEntities } from "./queries";

interface RefileDialogProps {
  memory: MemoryView;
  onClose: () => void;
  onDone: (moved: Refiled) => void;
}

/** What each kind is called beside a name in the list. */
const KIND_WORD = {
  person: "refile.option.person",
  place: "refile.option.place",
  thing: "refile.option.thing",
} as const satisfies Record<EntityKind, Parameters<typeof t>[0]>;

const KINDS: { kind: EntityKind; label: Parameters<typeof t>[0] }[] = [
  { kind: "person", label: "refile.kind.person" },
  { kind: "place", label: "refile.kind.place" },
  { kind: "thing", label: "refile.kind.thing" },
];

/** The most that are listed at once; typing narrows them. */
const SHOWN = 8;

/**
 * "Is this about something else?": moves one memory to another person, place or thing that
 * Nocturne already knows, or to a new one. Only this memory moves, and its words stay.
 */
export function RefileDialog({ memory, onClose, onDone }: RefileDialogProps) {
  const engine = useEngine();
  const [mode, setMode] = useState<"existing" | "new">("existing");
  const [query, setQuery] = useState("");
  const [chosen, setChosen] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [kind, setKind] = useState<EntityKind>("thing");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | undefined>();
  const search = useRef<HTMLInputElement>(null);
  const entities = useEntities(null, query.trim() === "" ? null : query.trim());

  const options = (entities.data ?? []).filter((e) => e.id !== memory.entityId);
  const ready = mode === "existing" ? chosen !== null : name.trim() !== "";

  const move = async () => {
    setBusy(true);
    setError(undefined);
    try {
      const moved = await engine.refileMemory(
        memory.id,
        mode === "existing" && chosen !== null
          ? { type: "existing", entityId: chosen }
          : { type: "new", name: name.trim(), kind },
      );
      onDone(moved);
    } catch {
      setError(t("memories.failed"));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog title={t("refile.title")} onClose={onClose} locked={busy} initialFocus={search}>
      <form
        className={styles.body}
        onSubmit={(event) => {
          event.preventDefault();
          if (ready) void move();
        }}
      >
        <p className={`t-page-line ${styles.sentence}`}>{memory.sentence}</p>
        <p>{t("refile.lead", { subject: memory.subject })}</p>

        <fieldset className={styles.modes}>
          <legend className={styles.legend}>{t("refile.mode.label")}</legend>
          <label className={styles.choice}>
            <input
              type="radio"
              name="refile-mode"
              checked={mode === "existing"}
              onChange={() => setMode("existing")}
            />
            {t("refile.mode.existing")}
          </label>
          <label className={styles.choice}>
            <input
              type="radio"
              name="refile-mode"
              checked={mode === "new"}
              onChange={() => setMode("new")}
            />
            {t("refile.mode.new")}
          </label>
        </fieldset>

        {mode === "existing" ? (
          <>
            <TextInput
              label={t("refile.search.label")}
              value={query}
              inputRef={search}
              onChange={setQuery}
            />
            {entities.isError ? (
              <p className={styles.problem} role="alert">
                {t("memories.error")}
              </p>
            ) : options.length === 0 ? (
              <p className={styles.note} role="status">
                {entities.data ? t("refile.none") : t("memories.loading")}
              </p>
            ) : (
              <fieldset className={styles.options}>
                <legend className="sr-only">{t("refile.list.label")}</legend>
                {options.slice(0, SHOWN).map((e) => (
                  <label key={e.id} className={styles.option}>
                    <input
                      type="radio"
                      name="refile-target"
                      checked={chosen === e.id}
                      onChange={() => setChosen(e.id)}
                    />
                    <span className={styles.optionName}>{e.name}</span>{" "}
                    <span className={styles.optionRole}>
                      {e.role ? `${t(KIND_WORD[e.kind])} · ${e.role}` : t(KIND_WORD[e.kind])}
                    </span>
                  </label>
                ))}
              </fieldset>
            )}
            {options.length > SHOWN ? <p className={styles.note}>{t("refile.more")}</p> : null}
          </>
        ) : (
          <>
            <TextInput
              label={t("refile.new.label")}
              value={name}
              inputRef={search}
              maxLength={120}
              onChange={setName}
            />
            <fieldset className={styles.kinds}>
              <legend className={styles.legend}>{t("refile.kind.label")}</legend>
              {KINDS.map((k) => (
                <label key={k.kind} className={styles.choice}>
                  <input
                    type="radio"
                    name="refile-kind"
                    checked={kind === k.kind}
                    onChange={() => setKind(k.kind)}
                  />
                  {t(k.label)}
                </label>
              ))}
            </fieldset>
          </>
        )}

        {error ? (
          <p className={styles.problem} role="alert">
            {error}
          </p>
        ) : null}
        <div className={styles.actions}>
          <Button variant="secondary" onClick={onClose} disabled={busy}>
            {t("dialog.cancel")}
          </Button>
          <Button type="submit" variant="primary" disabled={busy || !ready}>
            {t("refile.save")}
          </Button>
        </div>
      </form>
    </Dialog>
  );
}
