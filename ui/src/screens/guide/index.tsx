import { useQuery } from "@tanstack/react-query";
import { useEffect } from "react";
import { useQueryParam } from "../../app/query";
import { BackLink } from "../../components/BackLink/BackLink";
import { ScreenSheet } from "../../components/ScreenSheet/ScreenSheet";
import { t } from "../../copy/t";
import type { GuideBlock } from "../../engine/bindings";
import { useEngine } from "../../engine/context";
import styles from "./Guide.module.css";
import { GuideText } from "./GuideText";
import { type GuideFrom, guideHref } from "./links";

function Block({ block }: { block: GuideBlock }) {
  switch (block.type) {
    case "paragraph":
      return (
        <p>
          <GuideText parts={block.parts} />
        </p>
      );
    case "steps":
      return (
        <ol className={styles.steps}>
          {block.items.map((item, i) => (
            <li key={i}>
              <GuideText parts={item} />
            </li>
          ))}
        </ol>
      );
    case "list":
      return (
        <ul className={styles.list}>
          {block.items.map((item, i) => (
            <li key={i}>
              <GuideText parts={item} />
            </li>
          ))}
        </ul>
      );
    case "table": {
      const [head, ...rows] = block.rows;
      return (
        <table className={styles.table}>
          {head ? (
            <thead>
              <tr>
                {head.map((cell, i) => (
                  <th key={i} scope="col">
                    <GuideText parts={cell} />
                  </th>
                ))}
              </tr>
            </thead>
          ) : null}
          <tbody>
            {rows.map((row, r) => (
              <tr key={r}>
                {row.map((cell, i) => (
                  <td key={i}>
                    <GuideText parts={cell} />
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      );
    }
  }
}

/**
 * The Nocturne guide, read-only, one page at a time. Opened from an answer it scrolls to the
 * cited section and marks it, and Back says "Ask"; opened from Settings it shows the first page.
 */
export default function GuideScreen() {
  const engine = useEngine();
  const { data: guide } = useQuery({
    queryKey: ["guide"],
    queryFn: () => engine.getGuide(),
    staleTime: Number.POSITIVE_INFINITY,
  });
  const from: GuideFrom = useQueryParam("from") === "ask" ? "ask" : "settings";
  const pageSlug = useQueryParam("page");
  const sectionSlug = useQueryParam("section");
  const page = guide?.pages.find((p) => p.slug === pageSlug) ?? guide?.pages[0];

  useEffect(() => {
    if (!page) return;
    const target = sectionSlug ? document.getElementById(`guide-${sectionSlug}`) : null;
    if (target) target.scrollIntoView?.({ block: "start" });
    else window.scrollTo?.(0, 0);
  }, [page, sectionSlug]);

  const back =
    from === "ask" ? (
      <BackLink to="/ask" label={t("screen.ask.title")} />
    ) : (
      <BackLink to="/settings" label={t("screen.settings.title")} />
    );

  if (!guide || !page) {
    return (
      <ScreenSheet>
        {back}
        <p className={styles.loading}>{t("screen.loading")}</p>
      </ScreenSheet>
    );
  }

  return (
    <ScreenSheet>
      {back}
      <div className={styles.layout}>
        <article className={styles.page} aria-labelledby="guide-title">
          <p className={styles.eyebrow}>{t("screen.guide.title")}</p>
          <h1 id="guide-title" className={`t-page-title ${styles.title}`}>
            {page.title}
          </h1>
          <p className={styles.opening}>
            <GuideText parts={page.opening} />
          </p>
          {page.sections.map((section) => (
            <section
              key={section.slug}
              id={`guide-${section.slug}`}
              className={styles.section}
              data-marked={section.slug === sectionSlug}
              aria-labelledby={`guide-${section.slug}-heading`}
            >
              <h2 id={`guide-${section.slug}-heading`} className={styles.heading}>
                {section.heading}
              </h2>
              {section.blocks.map((block, i) => (
                <Block key={i} block={block} />
              ))}
            </section>
          ))}
        </article>
        <nav className={styles.contents} aria-label={t("screen.guide.title")}>
          <h2 className={styles.eyebrow}>{t("guide.page.contents")}</h2>
          <ul className={styles.links}>
            {page.sections.map((section) => (
              <li key={section.slug}>
                <a
                  href={guideHref(from, { page: page.slug, section: section.slug })}
                  aria-current={section.slug === sectionSlug ? "location" : undefined}
                >
                  {section.heading}
                </a>
              </li>
            ))}
          </ul>
          <h2 className={styles.eyebrow}>{t("guide.contents")}</h2>
          <ul className={styles.links}>
            {guide.pages.map((p) => (
              <li key={p.slug}>
                <a
                  href={guideHref(from, { page: p.slug })}
                  aria-current={p.slug === page.slug ? "page" : undefined}
                >
                  {p.title}
                </a>
              </li>
            ))}
          </ul>
          <p className={styles.version}>{t("guide.version", { version: guide.version })}</p>
        </nav>
      </div>
    </ScreenSheet>
  );
}
