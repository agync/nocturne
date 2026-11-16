import { findRoute } from "../../app/routes";
import { Button } from "../../components/Button/Button";
import { Icon } from "../../components/Icon/Icon";
import { PlaceIcon } from "../../components/KeepAskBar/PlaceIcon";
import { t } from "../../copy/t";
import type { GuideCite, HelpView } from "../../engine/bindings";
import { GuideText } from "../guide/GuideText";
import { guideHref } from "../guide/links";
import askStyles from "./Ask.module.css";
import styles from "./Help.module.css";

/**
 * The screen a guide section is about, for "Open {screen}": by section first, then by page. A
 * section about no one screen has none.
 */
const SCREENS: Record<string, string> = {
  "away-from-home": "/settings/away",
  "online-answers": "/settings/online",
  "desktops#quick-keep-shortcut": "/settings",
  "desktops#notifications": "/settings",
  "desktops#start-with-this-computer": "/settings",
  "desktops#a-folder-that-keeps-what-you-drop-in-it": "/settings",
  "your-data#where-it-is": "/privacy",
  "your-data#look-at-what-it-holds": "/memories",
  "your-data#take-a-copy": "/privacy",
  "your-data#back-up-and-restore": "/privacy",
  "your-data#let-go-bring-back-forget": "/fading",
  "your-data#delete-everything": "/privacy",
  "your-data#how-you-can-tell-nothing-leaves-your-computer": "/privacy",
};

export function screenFor(cite: GuideCite): string | null {
  return SCREENS[`${cite.page}#${cite.section}`] ?? SCREENS[cite.page] ?? null;
}

/** "Open Away from home", for the first cited section that is about a screen. */
function openScreen(help: HelpView): { path: string; label: string } | null {
  for (const cite of help.sections) {
    const path = screenFor(cite);
    const route = path ? findRoute(path) : undefined;
    if (path && route) return { path, label: t("ask.help.open", { screen: t(route.title) }) };
  }
  return null;
}

/** Where a help answer was written, above "You asked". Help is always written on this computer. */
export function HelpMark({ help, count }: { help: HelpView; count: number }) {
  const text = help.onlineOn
    ? count > 0
      ? t("ask.mark.local.guideAndKept", { count })
      : t("ask.mark.local.guide")
    : count > 0
      ? t("ask.mark.guideAndKept", { count })
      : t("ask.mark.guide");
  return (
    <p className={askStyles.mark} data-place="computer" data-help="true">
      {help.onlineOn ? <PlaceIcon place="computer" /> : null}
      <span>{text}</span>
    </p>
  );
}

/** A guide receipt as a card: the book, the section heading, the guide's sentence, its page. */
export function GuideReceiptCard({ cite, active }: { cite: GuideCite; active: boolean }) {
  return (
    <li className={askStyles.receipt} data-kind="guide" data-active={active} data-memory={cite.id}>
      <span className={`${askStyles.thumb} ${styles.book}`} aria-hidden="true">
        <Icon name="book-open" size={18} />
      </span>
      <a
        className={askStyles.receiptOpen}
        data-open
        href={guideHref("ask", { page: cite.page, section: cite.section })}
        aria-label={t("ask.help.receipt.open", { section: cite.heading })}
      >
        <span className={askStyles.receiptTitle}>{cite.heading}</span>
        <span className={askStyles.receiptSaid}>“{cite.quote}”</span>
        <span className={askStyles.receiptMeta}>
          {t("ask.help.receipt.meta", { page: cite.pageTitle })}
        </span>
      </a>
    </li>
  );
}

/** A guide receipt as a chip, as Earlier turns and the guide's own words show it. */
export function GuideReceiptChip({ cite }: { cite: GuideCite }) {
  return (
    <a
      className={`${styles.chip} hit`}
      href={guideHref("ask", { page: cite.page, section: cite.section })}
      aria-label={t("ask.help.receipt.open", { section: cite.heading })}
    >
      <Icon name="book-open" size={14} />
      <span>
        {cite.pageTitle} · {cite.heading}
      </span>
    </a>
  );
}

/** "Open {screen}" when a cited section is about one, and always "Open the guide". */
export function HelpButtons({ help }: { help: HelpView }) {
  const screen = openScreen(help);
  const first = help.sections[0];
  return (
    <div className={askStyles.buttons}>
      {screen ? (
        <Button
          variant="primary"
          onClick={() => {
            window.location.hash = `#${screen.path}`;
          }}
        >
          {screen.label}
        </Button>
      ) : null}
      <Button
        variant="secondary"
        onClick={() => {
          window.location.hash = guideHref(
            "ask",
            first ? { page: first.page, section: first.section } : null,
          );
        }}
      >
        {t("ask.help.openGuide")}
      </Button>
    </div>
  );
}

/**
 * The guide's own words, on a size of computer that has not passed the help check: the page's
 * opening sentence and the first sentence of each step, its receipts and both buttons.
 */
export function GuideWords({ help }: { help: HelpView }) {
  const opening = help.opening;
  const first = opening.length > 0 ? firstSentence(opening) : [];
  return (
    <section className={styles.card} aria-labelledby="help-label">
      <p id="help-label" className={styles.label}>
        <Icon name="book-open" size={14} />
        {t("ask.help.label")}
      </p>
      {first.length > 0 ? (
        <p className={styles.lead}>
          <GuideText parts={first} />
        </p>
      ) : null}
      <div className={styles.columns} data-count={help.sections.length}>
        {help.sections.map((cite) => (
          <div key={cite.id} className={styles.group}>
            <h2 className={styles.groupTitle}>{cite.heading}</h2>
            <ol className={styles.steps}>
              {cite.steps.map((step, i) => (
                <li key={i}>
                  <span>
                    <GuideText parts={step} />
                  </span>
                </li>
              ))}
            </ol>
          </div>
        ))}
      </div>
      <div className={styles.foot}>
        <HelpButtons help={help} />
        <ul className={styles.chips} aria-label={t("ask.help.receipts")}>
          {help.sections.map((cite) => (
            <li key={cite.id}>
              <GuideReceiptChip cite={cite} />
            </li>
          ))}
        </ul>
      </div>
    </section>
  );
}

/** "Did you mean help with Nocturne?" under an answer the guide may also cover. */
export function DidYouMean({ help, onShow }: { help: HelpView; onShow: () => void }) {
  const page = help.sections[0]?.pageTitle;
  return (
    <div className={askStyles.waitNote} data-help="unsure">
      <span className={askStyles.waitText}>
        <span className={askStyles.waitTitle}>{t("ask.help.didYouMean")}</span>
        {page ? (
          <span className={askStyles.waitHint}>{t("ask.help.didYouMean.hint", { page })}</span>
        ) : null}
      </span>
      <Button variant="secondary" onClick={onShow}>
        {t("ask.help.didYouMean.show")}
      </Button>
    </div>
  );
}

function firstSentence(parts: HelpView["opening"]): HelpView["opening"] {
  const out: HelpView["opening"] = [];
  for (const part of parts) {
    if (part.type === "text" || part.type === "strong") {
      const end = part.text.search(/[.!?](\s|$)/);
      if (end >= 0) {
        out.push({ ...part, text: part.text.slice(0, end + 1) });
        return out;
      }
    }
    out.push(part);
  }
  return out;
}
