import { ActionBar } from "../../components/ActionBar/ActionBar";
import { Button } from "../../components/Button/Button";
import { tc } from "../../components/ControlChip/copy";
import { Icon } from "../../components/Icon/Icon";
import { Toggle } from "../../components/Toggle/Toggle";
import { t } from "../../copy/t";
import type { StepProps } from "./draft";
import styles from "./Online.module.css";
import { Frame } from "./parts";
import { StepAside, StepLabel } from "./SetupBits";

/** O5: which questions may go online. Phones appear only when Away from home is on. */
export function WhatToSend({ draft, phonesAvailable, setDraft, onBack, onNext }: StepProps) {
  return (
    <Frame
      back={{ to: "/settings/online", label: t("screen.online.title") }}
      aside={
        <StepAside
          heading={t("online.send.aside.heading")}
          paragraphs={[t("online.send.aside.body"), t("online.send.aside.first")]}
        />
      }
    >
      <StepLabel step={3} />
      <h1 className={styles.stepTitle} tabIndex={-1}>
        {t("online.send.title")}
      </h1>
      <p className={styles.body}>{tc("online.send.lead")}</p>
      <ul className={styles.rows}>
        <li className={styles.row}>
          <span className={styles.textStack}>
            <span className={styles.name}>{t("online.send.here.label")}</span>
            <span className={styles.sub}>{t("online.send.here.hint")}</span>
          </span>
          <span className={styles.value}>{t("online.send.here.value")}</span>
        </li>
        {phonesAvailable ? (
          <li className={styles.row}>
            <Toggle
              label={t("online.send.phones.label")}
              hint={t("online.send.phones.hint")}
              checked={draft.phones}
              onChange={(phones) => setDraft({ ...draft, phones })}
            />
          </li>
        ) : null}
        <li className={styles.row}>
          <span className={styles.lockNote}>
            <Icon name="lock" size={18} />
            <span className={styles.textStack}>
              <span className={styles.name}>{t("online.send.private.label")}</span>
              <span className={styles.sub}>{t("online.send.private.hint")}</span>
            </span>
          </span>
        </li>
      </ul>
      <ActionBar line={t("online.bar.line")}>
        <Button variant="primary" onClick={onNext}>
          {t("online.continue")}
        </Button>
        <Button variant="quiet" onClick={onBack}>
          {t("online.back")}
        </Button>
      </ActionBar>
    </Frame>
  );
}
