import { useState } from "react";
import { Button } from "../../components/Button/Button";
import { useToast } from "../../components/Toast/Toast";
import { formatBytes } from "../../copy/format";
import { type CopyId, t } from "../../copy/t";
import type { SetupView, Tier } from "../../engine/bindings";
import { useEngine, useRefresh } from "../../engine/context";
import styles from "./Setup.module.css";

const NAME: Record<Exclude<Tier, "unsupported">, CopyId> = {
  large: "setup.tier.large",
  standard: "setup.tier.standard",
  small: "setup.tier.small",
};

const RESULT: Record<Exclude<Tier, "unsupported">, CopyId> = {
  large: "setup.result.large",
  standard: "setup.result.standard",
  small: "setup.result.small",
};

/** Why nothing fits: not enough memory, or not enough free space for the smallest. */
function shortfall(
  view: SetupView,
): { kind: "memory"; min: number } | { kind: "disk"; needed: number } {
  const smallest = [...view.options].sort((a, b) => a.minRamGib - b.minRamGib)[0];
  if (!smallest || view.ramGib < smallest.minRamGib) {
    return { kind: "memory", min: smallest?.minRamGib ?? 14 };
  }
  return { kind: "disk", needed: smallest.downloadBytes + 2e9 };
}

interface Props {
  view: SetupView;
  /** Called once the engine has started setting up. */
  onStarted: () => void;
}

/** H2, second half: what this computer can run, the three tiers, and the one button. */
export function Choosing({ view, onStarted }: Props) {
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
        <h1 id="result" className={`t-page-title ${styles.title}`}>
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
      <h1 id="result" className={`t-page-title ${styles.title}`} aria-live="polite">
        {t(headline)}
      </h1>
      <fieldset className={styles.tiers}>
        <legend className="sr-only">{t("setup.tier.label")}</legend>
        {view.options.map((o) => {
          const tier = o.tier as Exclude<Tier, "unsupported">;
          const here = o.tier === view.detected;
          const detail =
            o.tier === "large"
              ? "setup.tier.detail.fastest"
              : o.tier === "small"
                ? "setup.tier.detail.slower"
                : "setup.tier.detail";
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
                  {t(detail, { ram: o.minRamGib, size: formatBytes(o.downloadBytes) })}
                </span>
                {!o.fits ? (
                  <span className={styles.tierDetail}>{t("setup.tier.toobig")}</span>
                ) : null}
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
      <div className={styles.foot}>
        <Button
          variant="quiet"
          aria-expanded={why}
          aria-controls="why"
          onClick={() => setWhy(!why)}
        >
          {t("setup.why")}
        </Button>
        <Button variant="primary" disabled={busy || !chosen} onClick={() => void start()}>
          {t("setup.start", { size: formatBytes(chosen?.downloadBytes ?? 0) })}
        </Button>
      </div>
      {why ? (
        <p id="why" className={styles.why}>
          {t("setup.why.body", {
            ram: view.ramGib,
            large: ram("large"),
            standard: ram("standard"),
            small: ram("small"),
          })}
        </p>
      ) : null}
    </section>
  );
}
