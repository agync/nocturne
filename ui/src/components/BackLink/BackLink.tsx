import { useQueryParam } from "../../app/query";
import { t } from "../../copy/t";
import styles from "./BackLink.module.css";

interface BackLinkProps {
  /** Route path of the destination, such as "/settings". */
  to: string;
  label: string;
}

/**
 * Back to the screen this one belongs to, or to the answer when an answer opened it
 * (`from=ask`), so the person returns to what they asked.
 */
export function BackLink({ to, label }: BackLinkProps) {
  const fromAsk = useQueryParam("from") === "ask";
  const [href, words] = fromAsk ? ["/ask", t("screen.ask.title")] : [to, label];
  return (
    <a className={`${styles.back} hit`} href={`#${href}`}>
      <span aria-hidden="true">←</span>
      {words}
    </a>
  );
}

/** Back to the answer, on a screen that has no Back of its own, when an answer opened it. */
export function BackToAnswer() {
  const fromAsk = useQueryParam("from") === "ask";
  return fromAsk ? <BackLink to="/ask" label={t("screen.ask.title")} /> : null;
}
