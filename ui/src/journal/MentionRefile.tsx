import { useRef, useState } from "react";
import { Button } from "../components/Button/Button";
import { Dialog } from "../components/Dialog/Dialog";
import { TextInput } from "../components/TextInput/TextInput";
import { t } from "../copy/t";
import type { EntityKind, Mention } from "../engine/bindings";
import { useEngine } from "../engine/context";
import styles from "../screens/memories/Dialogs.module.css";
import { useEntities } from "../screens/memories/queries";

interface MentionRefileProps {
  mention: Mention;
  /** "Not a person" and its kin: the kind is chosen first. */
  kindFirst: boolean;
  onClose: () => void;
  onDone: (moved: Mention) => void;
}

const KIND_WORD = {
  person: "refile.option.person",
  place: "refile.option.place",
  thing: "refile.option.thing",
} as const satisfies Record<EntityKind, Parameters<typeof t>[0]>;

const KIND_LABEL = {
  person: "refile.kind.person",
  place: "refile.kind.place",
  thing: "refile.kind.thing",
} as const satisfies Record<EntityKind, Parameters<typeof t>[0]>;

const KINDS: EntityKind[] = ["person", "place", "thing"];
const SHOWN = 8;

/**
 * "Is this about something else?" for a name in an entry: moves only the memories from this entry
 * to someone Nocturne already knows or to a new one, through the mention rather than one memory.
 */
export function MentionRefile({ mention, kindFirst, onClose, onDone }: MentionRefileProps) {
  const engine = useEngine();
  const others = KINDS.filter((k) => k !== mention.kind);
  const [mode, setMode] = useState<"existing" | "new">("existing");
  const [query, setQuery] = useState("");
  const [chosen, setChosen] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [kind, setKind] = useState<EntityKind>(kindFirst ? (others[0] ?? "thing") : "thing");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | undefined>();
  const search = useRef<HTMLInputElement>(null);
  const first = useRef<HTMLInputElement>(null);
  const entities = useEntities(kindFirst ? kind : null, query.trim() === "" ? null : query.trim());

  const options = (entities.data ?? []).filter((e) => e.id !== mention.entityId);
  const ready = mode === "existing" ? chosen !== null : name.trim() !== "";
  const kinds = kindFirst ? others : KINDS;

  const move = async () => {
    setBusy(true);
    setError(undefined);
    try {
      const moved = await engine.refileMention(
        mention.id,
        mode === "existing" && chosen !== null
          ? { type: "existing", entityId: chosen }
          : { type: "new", name: name.trim(), kind },
      );
      onDone(moved);
    } catch {
      setError(t("memories.failed"));
      setBusy(false);
    }
  };

  const kindChoice = (
    <fieldset className={styles.kinds}>
      <legend className={styles.legend}>{t("refile.kind.label")}</legend>
      {kinds.map((k) => (
        <label key={k} className={styles.choice}>
          <input
            type="radio"
            name="mention-kind"
            ref={kindFirst && k === kinds[0] ? first : undefined}
            checked={kind === k}
            onChange={() => {
              setKind(k);
              setChosen(null);
            }}
          />
          {t(KIND_LABEL[k])}
        </label>
      ))}
    </fieldset>
  );

  return (
    <Dialog
      title={t("refile.title")}
      onClose={onClose}
      locked={busy}
      initialFocus={kindFirst ? first : search}
    >
      <form
        className={styles.body}
        onSubmit={(event) => {
          event.preventDefault();
          if (ready) void move();
        }}
      >
        <p>{t("journal.people.refile", { subject: mention.name })}</p>
        {kindFirst ? kindChoice : null}

        <fieldset className={styles.modes}>
          <legend className={styles.legend}>{t("refile.mode.label")}</legend>
          <label className={styles.choice}>
            <input
              type="radio"
              name="mention-refile-mode"
              checked={mode === "existing"}
              onChange={() => setMode("existing")}
            />
            {t("refile.mode.existing")}
          </label>
          <label className={styles.choice}>
            <input
              type="radio"
              name="mention-refile-mode"
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
                      name="mention-refile-target"
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
            {kindFirst ? null : kindChoice}
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
