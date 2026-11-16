import { useState } from "react";
import { Button } from "../../components/Button/Button";
import { tc } from "../../components/ControlChip/copy";
import { Chevron } from "../../components/Disclosure/Chevron";
import { useToast } from "../../components/Toast/Toast";
import { formatBytes } from "../../copy/format";
import { type CopyId, t } from "../../copy/t";
import type { SetupView, Tier } from "../../engine/bindings";
import { useEngine, useRefresh } from "../../engine/context";
import styles from "./Setup.module.css";
import { useArrivalFocus } from "./useArrivalFocus";

const NAME: Record<Exclude<Tier, "unsupported">, CopyId> = {
  large: "setup.tier.large",
  standard: "setup.tier.standard",
  small: "setup.tier.small",
  tiny: "setup.tier.tiny",
};

const RESULT: Record<Exclude<Tier, "unsupported">, CopyId> = {
  large: "setup.result.large",
  standard: "setup.result.standard",
  small: "setup.result.small",
  tiny: "setup.result.tiny",
};

/** What each size gives and costs, on a line of its own under its memory and download. */
const NOTE: Record<Exclude<Tier, "unsupported">, CopyId | null> = {
  large: "setup.tier.note.fastest",
  standard: null,
  small: "setup.tier.note.slower",
  tiny: "setup.tier.note.simpler",
};

const WHY_ORDER = ["large", "standard", "small", "tiny"] as const;

/** Why nothing fits: not enough memory, or not enough free space for the smallest. */
function shortfall(
  view: SetupView,
): { kind: "memory"; min: number } | { kind: "disk"; needed: number } {
  const smallest = [...view.options].sort((a, b) => a.minRamGib - b.minRamGib)[0];
  if (!smallest || view.ramGib < smallest.minRamGib) {
    return { kind: "memory", min: smallest?.minRamGib ?? 7 };
  }
  return { kind: "disk", needed: smallest.downloadBytes + 2e9 };
}

interface Props {
  view: SetupView;
  /** Called once the engine has started setting up. */
  onStarted: () => void;
}

/** H2, second half: what this computer can run, the four tiers, and the one button. */
export function Choosing({ view, onStarted }: Props) {
  const heading = useArrivalFocus();
  const engine = useEngine();
  const toast = useToast();
  const refresh = useRefresh();
  const fitting = view.options.filter((o) => o.fits);
  const unsupported = view.detected === "unsupported" || fitting.length === 0;
  const first = view.detected && view.detected !== "unsupported" ? view.detected : fitting[0]?.tier;
  const [picked, setPicked] = useState<Tier | undefined>(first);
  const [why, setWhy] = useState(false);
  const [busy, setBusy] = useState(false);

  if (unsupported) {
    const lack = shortfall(view);
    return (
      <section className={styles.card} aria-labelledby="result">
        <p className={`t-section ${styles.notYet}`}>{t("setup.step.one")}</p>
        <h1 ref={heading} tabIndex={-1} id="result" className={`t-page-title ${styles.title}`}>
          {t(
            lack.kind === "memory"
              ? "setup.result.unsupported.memory"
              : "setup.result.unsupported.disk",
          )}
        </h1>
        <p className={styles.lead}>
          {lack.kind === "memory"
            ? t("setup.unsupported.memory.detail", { ram: view.ramGib, min: lack.min })
            : t("setup.unsupported.disk.detail", {
                free: formatBytes(view.freeDiskBytes),
                needed: formatBytes(lack.needed),
              })}
        </p>
        <p className={styles.lead}>
          {t(
            lack.kind === "memory"
              ? "setup.unsupported.memory.next"
              : "setup.unsupported.disk.next",
          )}
        </p>
        <div className={styles.foot}>
          <Button variant="secondary" onClick={() => refresh("setup")}>
            {t("setup.check.again")}
          </Button>
        </div>
      </section>
    );
  }

  const chosen = view.options.find((o) => o.tier === picked) ?? fitting[0];
  const headline = RESULT[(chosen?.tier ?? "standard") as Exclude<Tier, "unsupported">];
  const ram = (tier: Tier) => view.options.find((o) => o.tier === tier)?.minRamGib ?? 0;

  const start = async () => {
    if (!chosen) return;
    setBusy(true);
    try {
      await engine.startSetup(chosen.tier);
      refresh("setup");
      onStarted();
    } catch {
      toast.show({ message: t("setup.action.failed") });
      setBusy(false);
    }
  };

  return (
    <section className={styles.card} aria-labelledby="result">
      <p className={`t-section ${styles.eyebrow}`}>{t("setup.step.one")}</p>
      <h1
        ref={heading}
        tabIndex={-1}
        id="result"
        className={`t-page-title ${styles.title}`}
        aria-live="polite"
      >
        {t(headline)}
      </h1>
      <fieldset className={styles.tiers}>
        <legend className="sr-only">{t("setup.tier.label")}</legend>
        {view.options.map((o) => {
          const tier = o.tier as Exclude<Tier, "unsupported">;
          const here = o.tier === view.detected;
          const tooBig = view.ramGib < o.minRamGib ? "setup.tier.toobig" : "setup.tier.toobig.disk";
          return (
            <label key={o.tier} className={styles.tier} data-disabled={!o.fits}>
              <input
                type="radio"
                name="tier"
                value={o.tier}
                className={styles.radio}
                checked={chosen?.tier === o.tier}
                disabled={!o.fits}
                onChange={() => setPicked(o.tier)}
              />
              <span className={styles.tierBody}>
                <span className={styles.tierName}>
                  {here ? t("setup.tier.here", { name: t(NAME[tier]) }) : t(NAME[tier])}
                </span>
                <span className={styles.tierDetail}>
                  {t("setup.tier.detail", { ram: o.minRamGib, size: formatBytes(o.downloadBytes) })}
                </span>
                {NOTE[tier] ? <span className={styles.tierDetail}>{t(NOTE[tier])}</span> : null}
                {!o.fits ? <span className={styles.tierDetail}>{t(tooBig)}</span> : null}
              </span>
            </label>
          );
        })}
      </fieldset>
      <p className={styles.lead}>
        {t("setup.needs", {
          size: formatBytes(chosen?.downloadBytes ?? 0),
          free: formatBytes(view.freeDiskBytes),
        })}
      </p>
      <p className={styles.lead}>{t("setup.needs.once")}</p>
      {chosen?.tier === "tiny" ? (
        <div className={styles.later}>
          <p>{t("setup.tiny.online")}</p>
          <p className={styles.where}>{tc("setup.tiny.online.where")}</p>
        </div>
      ) : null}
      <div className={styles.foot}>
        <Button
          variant="quiet"
          flush
          aria-expanded={why}
          aria-controls="why"
          onClick={() => setWhy(!why)}
        >
          <Chevron />
          {t("setup.why")}
        </Button>
        <Button variant="primary" disabled={busy || !chosen} onClick={() => void start()}>
          {t("setup.start", { size: formatBytes(chosen?.downloadBytes ?? 0) })}
        </Button>
      </div>
      {why ? (
        <div id="why" className={styles.why}>
          <p>{t("setup.why.lead")}</p>
          <p>{t("setup.why.here", { ram: view.ramGib })}</p>
          <table className={styles.sizes}>
            <tbody>
              {WHY_ORDER.map((tier) => (
                <tr key={tier}>
                  <th scope="row">{t(NAME[tier])}</th>
                  <td>{t("setup.why.row", { ram: ram(tier) })}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <p>{t("setup.why.close")}</p>
        </div>
      ) : null}
    </section>
  );
}
