import { Button } from "../../components/Button/Button";
import { ScreenSheet } from "../../components/ScreenSheet/ScreenSheet";
import { formatDay } from "../../copy/format";
import { t } from "../../copy/t";
import type { PageSummary } from "../../engine/bindings";
import { usePage, usePages } from "../../engine/context";
import styles from "./PastPages.module.css";

const MONTH = new Intl.DateTimeFormat("en-GB", { month: "long", year: "numeric" });

const monthOf = (date: string): string => MONTH.format(new Date(`${date}T12:00:00`));

/** Newest first, with the pages of each month under their own heading. */
function groups(pages: PageSummary[]): Array<{ month: string; pages: PageSummary[] }> {
  const out: Array<{ month: string; pages: PageSummary[] }> = [];
  for (const page of [...pages].sort((a, b) => b.date.localeCompare(a.date))) {
    const month = monthOf(page.date);
    const last = out.at(-1);
    if (last && last.month === month) last.pages.push(page);
    else out.push({ month, pages: [page] });
  }
  return out;
}

/** Serves #/pages. Opening one goes to #/pages/:date, which the morning page screen draws. */
export default function PastPages() {
  const pages = usePages();
  const { data: today } = usePage();

  if (pages.isError) {
    return (
      <div className={styles.message} role="alert">
        <p>{t("pages.error")}</p>
        <Button variant="secondary" onClick={() => void pages.refetch()}>
          {t("button.tryAgain")}
        </Button>
      </div>
    );
  }
  if (!pages.data) {
    return (
      <p className={styles.message} role="status">
        {t("pages.loading")}
      </p>
    );
  }

  return (
    <ScreenSheet>
      <h1 className="t-page-title">{t("pages.title")}</h1>
      <p className={`t-ui-body ${styles.dek}`}>{t("pages.dek")}</p>
      <hr className={styles.rule} />
      {pages.data.length === 0 ? (
        <p className={styles.empty}>{t("pages.empty")}</p>
      ) : (
        groups(pages.data).map((group) => (
          <section key={group.month} aria-labelledby={`month-${group.month}`}>
            <h2 id={`month-${group.month}`} className={`t-section ${styles.month}`}>
              {group.month}
            </h2>
            <ul className={styles.list}>
              {group.pages.map((page) => (
                <li key={page.date}>
                  <a className={styles.page} href={`#/pages/${page.date}`}>
                    <span className={styles.date}>
                      {today?.date === page.date ? (
                        <span className={styles.today}>{t("pages.today")}</span>
                      ) : null}
                      <span>{formatDay(page.date)}</span>
                    </span>
                    <span className={styles.headline}>{page.headline}</span>
                    <span className={styles.side}>
                      <span>{t("pages.number", { number: page.pageNumber })}</span>
                      {page.needsYou > 0 ? (
                        <span className={styles.needs}>
                          {t("pages.needsYou", { count: page.needsYou })}
                        </span>
                      ) : null}
                    </span>
                  </a>
                </li>
              ))}
            </ul>
          </section>
        ))
      )}
    </ScreenSheet>
  );
}
