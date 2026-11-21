import { useEffect, useRef, useState } from "react";
import { t } from "../../../copy/t";
import { Button, Page } from "../../screens/ui";
import type { NativeChannel, ThisPhoneSources } from "../api";
import { Group, Row } from "./chrome";
import { placesRich, useHeading } from "./parts";

/** "On · 3 calendars", "Not brought", or why it is not there. */
export function calendarValue(s: ThisPhoneSources, fresh = false): string {
  if (!s.computerTakes.includes("calendar")) return t("phone.app.value.needsNewer");
  switch (s.calendar) {
    case "stoppedOnComputer":
      return t("phone.app.value.stopped");
    case "needsNewerComputer":
      return t("phone.app.value.needsNewer");
    case "offInSettings":
      return t("phone.app.value.offInSettings");
    case "notBrought":
      return fresh ? t("phone.app.offer.choose") : t("phone.app.value.notBrought");
    default:
      return t("phone.app.value.calendars", { count: s.calendars });
  }
}

/** "On · 6 kinds", "Not brought", or why it is not there. */
export function healthValue(s: ThisPhoneSources, fresh = false): string {
  if (!s.computerTakes.includes("health")) return t("phone.app.value.needsNewer");
  switch (s.health) {
    case "stoppedOnComputer":
      return t("phone.app.value.stopped");
    case "needsNewerComputer":
      return t("phone.app.value.needsNewer");
    case "offInSettings":
      return t("phone.app.value.offInSettings");
    case "notBrought":
      return fresh ? t("phone.app.offer.choose") : t("phone.app.value.notBrought");
    default:
      return t("phone.app.value.kinds", { count: s.kinds });
  }
}

/** Whether the computer takes a source at all; a source it does not take is not offered. */
export const offered = (s: ThisPhoneSources, source: "calendar" | "health"): boolean =>
  s.computerTakes.includes(source) && s[source] !== "needsNewerComputer";

/**
 * "Bring more from this phone?": once after pairing. Both start off; each row opens its own
 * ask, and comes back here with what was chosen.
 */
export function Offer({
  channel,
  connected,
  chosen,
  onCalendar,
  onHealth,
  onDone,
}: {
  channel: NativeChannel;
  /**
   * What the computer takes comes in its hello. Until the app has connected both rows show;
   * once it has, a source the computer does not take is not offered.
   */
  connected: boolean;
  /** Rows the person opened already, which now say what was chosen in place of "Choose". */
  chosen: ReadonlySet<"calendar" | "health">;
  onCalendar(): void;
  onHealth(): void;
  onDone(): void;
}) {
  const [sources, setSources] = useState<ThisPhoneSources | null>(null);
  const heading = useHeading("offer");
  const done = useRef(onDone);
  done.current = onDone;

  useEffect(() => {
    let gone = false;
    channel.sources().then(
      (s) => {
        if (gone) return;
        if (connected && !offered(s, "calendar") && !offered(s, "health")) return done.current();
        setSources(connected ? s : { ...s, computerTakes: ["calendar", "health"] });
      },
      () => !gone && done.current(),
    );
    return () => {
      gone = true;
    };
  }, [channel, connected]);

  if (!sources) return null;
  return (
    <Page bar={null}>
      <main className="ph-main ph-offer">
        <h1 className="ph-h1 ph-h1--big" tabIndex={-1} ref={heading}>
          {t("phone.app.offer.title")}
        </h1>
        <p className="ph-body">{placesRich("phone.app.offer.lead")}</p>
        <Group>
          {offered(sources, "calendar") ? (
            <Row
              icon="calendar"
              label={t("phone.app.offer.calendar")}
              hint={t("phone.app.offer.calendar.hint")}
              value={calendarValue(sources, !chosen.has("calendar"))}
              onPress={onCalendar}
            />
          ) : null}
          {offered(sources, "health") ? (
            <Row
              icon="heart"
              label={t("phone.app.offer.health")}
              hint={t("phone.app.offer.health.hint")}
              value={healthValue(sources, !chosen.has("health"))}
              onPress={onHealth}
            />
          ) : null}
        </Group>
        <div className="ph-spacer" />
        <div className="ph-stack">
          <Button kind="primary" onClick={onDone}>
            {t("phone.app.offer.done")}
          </Button>
          <Button kind="quiet" onClick={onDone}>
            {t("phone.app.offer.notNow")}
          </Button>
        </div>
      </main>
    </Page>
  );
}
