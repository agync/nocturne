import { useCallback, useEffect, useState } from "react";
import { hashHref, useQueryParam } from "../../app/query";
import { t } from "../../copy/t";
import type { EntityKind } from "../../engine/bindings";
import { EntityList } from "./EntityList";
import { EntityPage } from "./EntityPage";
import styles from "./Memories.module.css";
import { useEntity } from "./queries";
import { Results } from "./Results";

/** #/memories and #/memories/:entityId: the list on the left, what it remembers on the right. */
export function Browser({ entityId }: { entityId: string | null }) {
  const query = useQueryParam("q") ?? "";
  const [kind, setKind] = useState<EntityKind>("person");
  const entity = useEntity(entityId);

  // Opening a place from a link puts the Places tab under it.
  useEffect(() => {
    if (entity.data) setKind(entity.data.kind);
  }, [entity.data]);

  const onQuery = useCallback(
    (value: string) => {
      // Typing replaces the address, so Back does not step through every letter.
      window.location.replace(
        hashHref(entityId ? `/memories/${entityId}` : "/memories", { q: value.trim() }),
      );
    },
    [entityId],
  );

  return (
    <div className={styles.frame}>
      <div className={styles.card} data-entity={entityId !== null}>
        {entityId === null ? (
          <header className={styles.top}>
            <h1 className="t-page-title">{t("memories.title")}</h1>
            <p className={styles.dek}>{t("memories.dek")}</p>
          </header>
        ) : null}
        <div className={styles.columns}>
          <div className={styles.left} data-hidden-narrow={entityId !== null}>
            <EntityList
              kind={kind}
              onKind={setKind}
              query={query}
              currentId={entityId}
              onQuery={onQuery}
            />
          </div>
          <div className={styles.right}>
            {entityId !== null ? (
              <EntityPage key={entityId} id={entityId} />
            ) : query ? (
              <Results query={query} />
            ) : (
              <p className={styles.pane}>{t("memories.pane.empty")}</p>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
