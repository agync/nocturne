import { useQueries } from "@tanstack/react-query";
import { useState } from "react";
import { t } from "../../copy/t";
import type { CheckIn, Glance, GlanceKind, KeptCount, NameRef } from "../../engine/bindings";
import { useEngine } from "../../engine/context";
import { FeelingMark } from "../../journal/FeelingMark";
import styles from "./Lookback.module.css";
import { joined, rich } from "./Receipts";
import { clock, entityHref, isPlace, itemHref, wordsOf } from "./text";

const KIND_COPY = {
  note: "journal.glance.kind.note",
  voice: "journal.glance.kind.voice",
  photo: "journal.glance.kind.photo",
  document: "journal.glance.kind.document",
  web: "journal.glance.kind.web",
} as const satisfies Record<GlanceKind, string>;

/** The items a count stands for, opened in place so each can be followed. */
function KeptItems({ ids }: { ids: readonly string[] }) {
  const engine = useEngine();
  const items = useQueries({
    queries: ids.map((id) => ({ queryKey: ["item", id], queryFn: () => engine.getItem(id) })),
  });
  return (
    <ul className={styles.keptItems} aria-label={t("journal.lookback.kept.items")}>
      {items.map((query, index) => {
        const id = ids[index] ?? "";
        return (
          <li key={id}>
            <a href={itemHref(id)}>{query.data?.title ?? "…"}</a>
          </li>
        );
      })}
    </ul>
  );
}

function KeptLine({ kept }: { kept: readonly KeptCount[] }) {
  const [open, setOpen] = useState<GlanceKind | null>(null);
  const total = kept.reduce((n, k) => n + k.itemIds.length, 0);
  const kinds = kept.map((k) => {
    const words = t(KIND_COPY[k.kind], { count: k.itemIds.length });
    const only = k.itemIds.length === 1 ? k.itemIds[0] : undefined;
    return only ? (
      <a key={k.kind} className={styles.count} href={itemHref(only)}>
        {words}
      </a>
    ) : (
      <button
        key={k.kind}
        type="button"
        className={styles.count}
        aria-expanded={open === k.kind}
        onClick={() => setOpen(open === k.kind ? null : k.kind)}
      >
        {words}
      </button>
    );
  });
  const shown = kept.find((k) => k.kind === open);
  return (
    <li>
      <p>{rich("journal.glance.kept", { count: total, kinds: joined(kinds) })}</p>
      {shown ? <KeptItems ids={shown.itemIds} /> : null}
    </li>
  );
}

function FeltLine({ felt }: { felt: readonly CheckIn[] }) {
  const items = felt.map((c) => {
    const href = itemHref(c.id);
    return (
      <span key={c.id} className={styles.feltItem} data-part={c.feeling.part}>
        <FeelingMark mark={c.feeling.part} size={13} />
        {rich("journal.glance.feltItem", {
          words: <a href={href}>{wordsOf(c)}</a>,
          time: <a href={href}>{clock(c.keptAt)}</a>,
        })}
      </span>
    );
  });
  return (
    <li>
      <p>{rich("journal.glance.felt", { items: joined(items) })}</p>
    </li>
  );
}

function Name({ name }: { name: NameRef }) {
  const link = <a href={entityHref(name.entityId)}>{name.name}</a>;
  return name.new ? <span>{rich("journal.glance.newName", { name: link })}</span> : link;
}

function WithLine({ names }: { names: readonly NameRef[] }) {
  const people = names.filter((n) => !isPlace(n));
  const places = names.filter(isPlace);
  return (
    <li>
      <p>
        {people.length > 0
          ? rich("journal.glance.with", {
              names: joined(people.map((n) => <Name key={n.entityId} name={n} />)),
            })
          : null}
        {people.length > 0 && places.length > 0 ? " " : null}
        {places.length > 0
          ? rich("journal.glance.at", {
              names: joined(places.map((n) => <Name key={n.entityId} name={n} />)),
            })
          : null}
      </p>
    </li>
  );
}

/** The day or period with no reading at all: counts, words, good things, names, first lines. */
export function GlanceView({ glance }: { glance: Glance }) {
  return (
    <section className={styles.section} aria-labelledby="glance-heading">
      <h2 id="glance-heading" className={styles.label}>
        {t("journal.glance.heading")}
      </h2>
      <ul className={styles.glance}>
        {glance.kept.length > 0 ? <KeptLine kept={glance.kept} /> : null}
        {glance.felt.length > 0 ? <FeltLine felt={glance.felt} /> : null}
        {glance.good.map((g) => (
          <li key={g.id} className={styles.goodLine}>
            <FeelingMark mark="sun" size={16} />
            <a href={itemHref(g.id)}>{g.text}</a>
          </li>
        ))}
        {glance.with.length > 0 ? <WithLine names={glance.with} /> : null}
        {glance.entryLines.length > 0 ? (
          <li>
            {glance.entryItemId ? (
              <a href={itemHref(glance.entryItemId)}>{`“${glance.entryLines.join(" ")}”`}</a>
            ) : (
              <p>{`“${glance.entryLines.join(" ")}”`}</p>
            )}
          </li>
        ) : null}
      </ul>
    </section>
  );
}
