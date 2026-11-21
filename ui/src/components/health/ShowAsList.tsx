import { type ReactNode, useId, useState } from "react";
import { t } from "../../copy/t";
import styles from "./Charts.module.css";

export interface ListRow {
  key: string;
  /** One cell for each column; the first may be a link to the reading it came from. */
  cells: ReactNode[];
  /** A reading removed in Health: in the secondary ink. */
  removed?: boolean;
}

interface ShowAsListProps {
  /** The chart's sentence: shown under the chart, and the name of the list. */
  sentence: string;
  /** The chart itself, drawn with the same sentence as its accessible name. */
  chart: ReactNode;
  columns: readonly string[];
  rows: readonly ListRow[];
  /** Extra words after the sentence, such as the fixed line about numbers. */
  children?: ReactNode;
}

/** A chart with its sentence and "Show as a list": the same numbers, by day, as a table. */
export function ShowAsList({ sentence, chart, columns, rows, children }: ShowAsListProps) {
  const [list, setList] = useState(false);
  const id = useId();
  return (
    <figure className={styles.figure}>
      {list ? (
        <table className={styles.table} id={id}>
          <caption>{sentence}</caption>
          <thead>
            <tr>
              {columns.map((c) => (
                <th key={c} scope="col">
                  {c}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr key={row.key} className={row.removed ? styles.removed : undefined}>
                {row.cells.map((cell, i) => (
                  <td key={i}>{cell}</td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      ) : (
        chart
      )}
      <p className={styles.say}>
        {list ? null : <>{sentence} </>}
        <button
          type="button"
          className={styles.toggle}
          aria-expanded={list}
          aria-controls={id}
          onClick={() => setList(!list)}
        >
          {list ? t("health.list.hide") : t("health.list.show")}
        </button>
      </p>
      {children}
    </figure>
  );
}
