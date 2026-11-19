import { useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Button } from "../components/Button/Button";
import { TextInput } from "../components/TextInput/TextInput";
import { useToast } from "../components/Toast/Toast";
import { t } from "../copy/t";
import type { EntityKind, Mention } from "../engine/bindings";
import { useEngine, useRefresh } from "../engine/context";
import styles from "./MentionCard.module.css";
import { MentionRefile } from "./MentionRefile";
import { NameIcon } from "./NameIcon";

interface MentionCardProps {
  mention: Mention;
  /** The chip that opened the card. The card sits under it and gives focus back on close. */
  anchor: HTMLElement | null;
  onClose: () => void;
}

const WRONG_KIND = {
  person: "journal.people.notPerson",
  place: "journal.people.notPlace",
  thing: "journal.people.notThing",
} as const satisfies Record<EntityKind, Parameters<typeof t>[0]>;

const WIDTH = 380;
const GAP = 8;

/**
 * What the journal found in an entry, in one small card: confirm it, file it under someone
 * else, say it is another kind of thing, or say the entry is not about it at all.
 */
export function MentionCard({ mention, anchor, onClose }: MentionCardProps) {
  const engine = useEngine();
  const toast = useToast();
  const refresh = useRefresh();
  const card = useRef<HTMLDivElement>(null);
  const [role, setRole] = useState("");
  const [busy, setBusy] = useState(false);
  const [refile, setRefile] = useState<{ kindFirst: boolean } | null>(null);
  const [place, setPlace] = useState<{ top: number; left: number } | null>(null);

  const close = () => {
    onClose();
    if (anchor?.isConnected) anchor.focus();
  };

  useLayoutEffect(() => {
    if (!anchor) return;
    const put = () => {
      const rect = anchor.getBoundingClientRect();
      const width = Math.min(WIDTH, window.innerWidth - 32);
      const height = card.current?.offsetHeight ?? 0;
      const below = rect.bottom + GAP;
      const room = window.innerHeight - 16;
      let top = below;
      if (below + height > room) {
        const above = rect.top - GAP - height;
        top = above >= 16 ? above : Math.max(16, room - height);
      }
      setPlace({
        top,
        left: Math.max(16, Math.min(rect.left, window.innerWidth - width - 16)),
      });
    };
    put();
    window.addEventListener("resize", put);
    return () => window.removeEventListener("resize", put);
  }, [anchor]);

  useLayoutEffect(() => {
    if (refile || !place) return;
    const first = card.current?.querySelector<HTMLElement>("input, button");
    first?.focus({ preventScroll: true });
  }, [refile, place]);

  useLayoutEffect(() => {
    if (refile) return;
    const away = (event: PointerEvent) => {
      const target = event.target as Node;
      if (card.current?.contains(target) || anchor?.contains(target)) return;
      onClose();
    };
    document.addEventListener("pointerdown", away);
    return () => document.removeEventListener("pointerdown", away);
  }, [refile, anchor, onClose]);

  const failed = () => toast.show({ message: t("memories.failed") });

  const confirm = async () => {
    setBusy(true);
    try {
      await engine.confirmMention(mention.id, mention.new && role.trim() ? role.trim() : null);
      refresh("journal");
      toast.show({ message: t("journal.people.confirmed", { name: mention.name }) });
      onClose();
    } catch {
      failed();
      setBusy(false);
    }
  };

  const remove = async () => {
    setBusy(true);
    try {
      await engine.removeMention(mention.id);
      refresh("journal");
      toast.show({
        message: t("journal.people.removed", { name: mention.name, kind: mention.kind }),
        action: {
          label: t("journal.timeline.undo"),
          onAction: () => {
            engine.restoreMention(mention.id).then(() => refresh("journal"), failed);
          },
        },
      });
      onClose();
    } catch {
      failed();
      setBusy(false);
    }
  };

  if (refile) {
    return (
      <MentionRefile
        mention={mention}
        kindFirst={refile.kindFirst}
        onClose={close}
        onDone={(moved) => {
          refresh("journal");
          toast.show({ message: t("refile.done", { name: moved.name }) });
          onClose();
        }}
      />
    );
  }

  const count = mention.memoryCount;
  const meta = mention.role
    ? t("journal.people.knownMeta", { role: mention.role, count })
    : t("journal.people.countOnly", { count });

  return createPortal(
    <div
      ref={card}
      className={styles.card}
      role="dialog"
      aria-label={mention.name}
      style={place ? { top: place.top, left: place.left } : { visibility: "hidden" }}
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.stopPropagation();
          close();
        }
      }}
    >
      <div className={styles.head}>
        <NameIcon kind={mention.kind} size={16} />
        <strong className={styles.name}>{mention.name}</strong>
        {mention.new ? <span className={styles.newTag}>{t("journal.people.new")}</span> : null}
      </div>
      {mention.new ? (
        <>
          <p className={styles.line}>{t("journal.people.newCard", { name: mention.name })}</p>
          <TextInput
            label={t("journal.people.role", { name: mention.name })}
            placeholder={t("journal.people.rolePlaceholder")}
            value={role}
            maxLength={120}
            onChange={setRole}
          />
        </>
      ) : (
        <p className={styles.line}>{meta}</p>
      )}
      {mention.new ? null : (
        <a className={styles.open} href={`#/memories/${mention.entityId}`}>
          {t("journal.people.open", { name: mention.name })}
        </a>
      )}
      <div className={styles.actions}>
        <Button variant="primary" disabled={busy} onClick={() => void confirm()}>
          {t("journal.people.confirm")}
        </Button>
        <Button variant="secondary" disabled={busy} onClick={() => setRefile({ kindFirst: false })}>
          {t("journal.people.someoneElse")}
        </Button>
        <Button variant="quiet" disabled={busy} onClick={() => setRefile({ kindFirst: true })}>
          {t(WRONG_KIND[mention.kind])}
        </Button>
        <Button variant="quiet" disabled={busy} onClick={() => void remove()}>
          {t("journal.people.remove", { name: mention.name })}
        </Button>
      </div>
    </div>,
    document.body,
  );
}
