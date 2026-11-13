import { type ReactNode, useEffect, useState } from "react";
import { hashHref } from "../../app/query";
import { Icon } from "../../components/Icon/Icon";
import { Tabs } from "../../components/Tabs/Tabs";
import { t } from "../../copy/t";
import type { EntityKind, EntityView } from "../../engine/bindings";
import styles from "./Memories.module.css";
import { useEntities } from "./queries";

const KINDS: EntityKind[] = ["person", "place", "thing"];

const TAB_LABEL = {
  person: "memories.tab.person",
  place: "memories.tab.place",
  thing: "memories.tab.thing",
} as const;

const EMPTY = {
  person: "memories.empty.person",
  place: "memories.empty.place",
  thing: "memories.empty.thing",
} as const;

interface EntityListProps {
  kind: EntityKind;
  onKind: (kind: EntityKind) => void;
  query: string;
  currentId: string | null;
  onQuery: (query: string) => void;
}

function Names({
  kind,
  query,
  currentId,
}: {
  kind: EntityKind;
  query: string;
  currentId: string | null;
}): ReactNode {
  const entities = useEntities(kind, query || null);
  if (entities.isError) return <p className={styles.empty}>{t("memories.error")}</p>;
  if (!entities.data) return <p className={styles.empty}>{t("memories.loading")}</p>;
  if (entities.data.length === 0) {
    return (
      <p className={styles.empty}>
        {query ? t("memories.empty.search", { query }) : t(EMPTY[kind])}
      </p>
    );
  }
  return (
    <ul className={styles.names}>
      {entities.data.map((e: EntityView) => (
        <li key={e.id}>
          <a
            className={styles.nameLink}
            href={hashHref(`/memories/${e.id}`, { q: query })}
            aria-current={e.id === currentId ? "page" : undefined}
          >
            <span className={styles.nameText}>
              <span className={styles.nameMain}>{e.name}</span>
              {e.role ? <span className={styles.nameRole}>{e.role}</span> : null}
            </span>
            <span className={styles.nameCount}>
              {t("memories.entity.count", { count: e.memoryCount })}
              <span className="sr-only">
                {" "}
                {t("memories.entity.summary", { count: e.memoryCount, items: e.itemCount })}
              </span>
            </span>
          </a>
        </li>
      ))}
    </ul>
  );
}

/** Search, the People, Places and Things tabs, and the names under them. */
export function EntityList({ kind, onKind, query, currentId, onQuery }: EntityListProps) {
  const [draft, setDraft] = useState(query);

  // The address bar is the source of truth: a link to ?q= fills the box.
  useEffect(() => setDraft(query), [query]);

  useEffect(() => {
    if (draft === query) return;
    const timer = setTimeout(() => onQuery(draft), 250);
    return () => clearTimeout(timer);
  }, [draft, query, onQuery]);

  const panel = <Names kind={kind} query={query} currentId={currentId} />;

  return (
    <div className={styles.list}>
      <div className={styles.search}>
        <Icon name="search" size={16} />
        <input
          type="search"
          className={styles.searchInput}
          aria-label={t("memories.search.label")}
          placeholder={t("memories.search.label")}
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") onQuery(draft);
          }}
        />
      </div>
      <Tabs
        label={t("memories.tabs.label")}
        value={kind}
        onChange={(id) => onKind(id as EntityKind)}
        tabs={KINDS.map((k) => ({ id: k, label: t(TAB_LABEL[k]), panel }))}
      />
      <a className={styles.days} href="#/pages">
        <span>{t("memories.days")}</span>
        <span className={styles.nameRole}>{t("memories.days.hint")}</span>
      </a>
    </div>
  );
}
