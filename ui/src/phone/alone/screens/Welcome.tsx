import { t } from "../../../copy/t";
import { useHeading } from "../../native/screens/parts";
import { Button, Icon, Page } from "../../screens/ui";
import { chips } from "./parts";

/** The first screen: start on this phone, connect to a computer, or bring back a backup. */
export function AloneWelcome({
  onStart,
  onConnect,
  onBackup,
}: {
  onStart(): void;
  onConnect(): void;
  onBackup(): void;
}) {
  const heading = useHeading("welcome");
  return (
    <Page bar={null}>
      <main className="ph-main">
        <div className="ph-hero">
          <span className="ph-mark" aria-hidden="true" />
          <h1 className="ph-h1" tabIndex={-1} ref={heading}>
            {t("phone.alone.welcome.title")}
          </h1>
          <p className="ph-body">{t("phone.alone.welcome.lead")}</p>
        </div>
        <div className="ph-spacer" />
        <div className="ph-stack">
          <Button kind="primary" onClick={onStart}>
            {t("phone.alone.welcome.start")}
          </Button>
          <Button onClick={onConnect}>
            <Icon name="laptop" size={20} />
            {t("phone.alone.welcome.connect")}
          </Button>
          <button type="button" className="ph-link ph-link--center" onClick={onBackup}>
            {t("phone.alone.welcome.backup")}
          </button>
          <p className="ph-small ph-center">{t("phone.alone.welcome.foot")}</p>
        </div>
      </main>
    </Page>
  );
}

/** "Your memory lives on this phone": what works now, what waits for a mind, then Start. */
export function Start({
  busy,
  failed,
  onStart,
}: {
  busy: boolean;
  failed: boolean;
  onStart(): void;
}) {
  const heading = useHeading("start");
  const works = [
    t("phone.alone.start.works.keep"),
    t("phone.alone.start.works.journal"),
    t("phone.alone.start.works.calendar"),
    t("phone.alone.start.works.find"),
  ];
  const waits = [
    t("phone.alone.start.waits.photos"),
    t("phone.alone.start.waits.page"),
    t("phone.alone.start.waits.answers"),
    t("phone.alone.start.waits.reflections"),
  ];
  return (
    <Page bar={null}>
      <main className="ph-main" style={{ gap: 14 }}>
        <div className="ph-hero" style={{ paddingTop: 20 }}>
          <span className="ph-hero-icon">
            <Icon name="phone" size={32} />
          </span>
          <h1 className="ph-steph" tabIndex={-1} ref={heading}>
            {t("phone.alone.start.title")}
          </h1>
        </div>
        <h2 className="ph-see-head">{t("phone.alone.start.works")}</h2>
        <ul className="ph-see">
          {works.map((w) => (
            <li key={w}>
              <Icon name="check" size={18} />
              <span>{w}</span>
            </li>
          ))}
        </ul>
        <h2 className="ph-see-head">{t("phone.alone.start.waits")}</h2>
        <ul className="ph-see ph-see--no">
          {waits.map((w) => (
            <li key={w}>
              <Icon name="moon" size={18} />
              <span>{w}</span>
            </li>
          ))}
        </ul>
        <p className="ph-small">{t("phone.alone.start.waitsHint")}</p>
        <p className="ph-limit">{chips("phone.alone.start.lostPhone")}</p>
      </main>
      <footer className="ph-foot">
        {failed ? (
          <p className="ph-error" role="alert">
            {t("phone.alone.start.failed")}
          </p>
        ) : null}
        <Button kind="primary" disabled={busy} onClick={onStart}>
          {failed ? t("phone.tryAgain") : t("phone.alone.start.button")}
        </Button>
      </footer>
    </Page>
  );
}

/** The memory could not be opened: it was put aside, and a backup can come back. */
export function Damaged({ onBackup, onHelp }: { onBackup(): void; onHelp(): void }) {
  const heading = useHeading("damaged");
  return (
    <Page bar={null}>
      <main className="ph-main">
        <div className="ph-hero" style={{ paddingTop: 72 }} role="alert">
          <span className="ph-hero-icon">
            <Icon name="info" size={36} />
          </span>
          <h1 className="ph-h1" tabIndex={-1} ref={heading}>
            {t("phone.alone.damaged.title")}
          </h1>
          <p className="ph-body">{t("phone.alone.damaged.body")}</p>
        </div>
        <div className="ph-spacer" />
        <div className="ph-stack">
          <Button kind="primary" onClick={onBackup}>
            {t("phone.alone.welcome.backup")}
          </Button>
          <button type="button" className="ph-textbtn" onClick={onHelp}>
            {t("phone.alone.damaged.help")}
          </button>
        </div>
      </main>
    </Page>
  );
}

/** Give Nocturne a mind: the one way in this release, Nocturne on a computer of one's own. */
export function GiveMind({
  backLabel,
  onBack,
  onScan,
}: {
  backLabel: string;
  onBack(): void;
  onScan(): void;
}) {
  const heading = useHeading("mind");
  const steps = [
    t("phone.alone.mind.step1"),
    chips("phone.alone.mind.step2"),
    t("phone.alone.mind.step3"),
  ];
  return (
    <Page
      bar={
        <header className="ph-bar ph-bar--back">
          <button
            type="button"
            className="ph-ib ph-ib--ink"
            onClick={onBack}
            aria-label={backLabel}
          >
            <Icon name="back" />
          </button>
        </header>
      }
    >
      <main className="ph-main" style={{ gap: 14 }}>
        <h1 className="ph-steph" tabIndex={-1} ref={heading}>
          {t("phone.alone.mind.title")}
        </h1>
        <p className="ph-body ph-ink2">{t("phone.alone.mind.lead")}</p>
        <ol className="ph-see">
          {steps.map((step, i) => (
            <li key={String(i)}>
              <span className="ph-see-num" aria-hidden="true">
                {i + 1}
              </span>
              <span>{step}</span>
            </li>
          ))}
        </ol>
      </main>
      <footer className="ph-foot">
        <Button kind="primary" onClick={onScan}>
          <Icon name="camera" size={20} />
          {t("phone.alone.mind.scan")}
        </Button>
        <button type="button" className="ph-textbtn" onClick={onBack}>
          {t("phone.alone.mind.notNow")}
        </button>
      </footer>
    </Page>
  );
}
