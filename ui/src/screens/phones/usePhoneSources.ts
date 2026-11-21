import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";
import { addDays, formatDayMonth, formatTime12, formatWeekday, localDay } from "../../copy/format";
import { t } from "../../copy/t";
import type {
  PhonePageOptions,
  PhoneSource,
  PhoneSources,
  PhoneSourcesView,
  ReadingKind,
} from "../../engine/bindings";
import { useEngine } from "../../engine/context";

export const PHONE_SOURCES_KEY = ["phoneSources"] as const;

/** From your phones as the computer last said it, and the changes the person can make to it. */
export function usePhoneSources() {
  const engine = useEngine();
  const client = useQueryClient();
  const query = useQuery({
    queryKey: PHONE_SOURCES_KEY,
    queryFn: () => engine.getPhoneSources(),
    staleTime: 5_000,
  });
  const apply = useCallback(
    (view: PhoneSourcesView) => client.setQueryData(PHONE_SOURCES_KEY, view),
    [client],
  );
  const setSource = useCallback(
    async (deviceId: string, source: PhoneSource, taking: boolean) =>
      apply(await engine.setPhoneSource(deviceId, source, taking)),
    [engine, apply],
  );
  const setPageOptions = useCallback(
    async (options: PhonePageOptions) => apply(await engine.setPhonePageOptions(options)),
    [engine, apply],
  );
  const setNoticing = useCallback(
    async (on: boolean) => {
      await engine.setNoticing(on);
      client.setQueryData<PhoneSourcesView>(PHONE_SOURCES_KEY, (old) =>
        old ? { ...old, noticing: on } : old,
      );
      void client.invalidateQueries({ queryKey: PHONE_SOURCES_KEY });
    },
    [engine, client],
  );
  const reread = useCallback(
    () => client.invalidateQueries({ queryKey: PHONE_SOURCES_KEY }),
    [client],
  );
  return {
    ...query,
    phones: query.data?.phones as PhoneSources[] | undefined,
    setSource,
    setPageOptions,
    setNoticing,
    reread,
  };
}

const KIND_WORD: Record<ReadingKind, Parameters<typeof t>[0]> = {
  sleep: "phones.kindList.sleep",
  heartRate: "phones.kindList.heartRate",
  restingHeartRate: "phones.kindList.restingHeartRate",
  bloodPressure: "phones.kindList.bloodPressure",
  steps: "phones.kindList.steps",
  workout: "phones.kindList.workout",
  weight: "phones.kindList.weight",
};

/** "Sleep, heart rate, steps": the kinds as a list that starts a sentence. */
export function kindsStarting(kinds: ReadingKind[]): string {
  const words = kinds.map((k) => t(KIND_WORD[k]));
  return words.map((w, i) => (i === 0 ? w : w.charAt(0).toLowerCase() + w.slice(1))).join(", ");
}

/** "sleep, heart rate and steps": the kinds inside a sentence. */
export function kindsInline(kinds: ReadingKind[]): string {
  const words = kinds.map((k) => t(KIND_WORD[k]).toLowerCase());
  return new Intl.ListFormat("en-GB", { style: "long", type: "conjunction" }).format(words);
}

/**
 * When something came in: "7:42 am" for today in a sentence, "yesterday 9:10 pm", "Wed 7:58 am"
 * within the week, or "13 Nov 7:02 am". A list starts with a capital and says "Today".
 */
export function cameInWhen(at: string, start: "sentence" | "list", now: Date = new Date()): string {
  const time = formatTime12(at);
  const day = localDay(at);
  const today = localDay(now);
  let text: string;
  if (day === today) {
    text = start === "sentence" ? time : t("phones.when.today", { time });
  } else if (day === addDays(today, -1)) {
    text = t("phones.when.yesterday", { time });
  } else {
    const recent = day > addDays(today, -7) && day < today;
    text = t("phones.when.other", { day: recent ? formatWeekday(at) : formatDayMonth(at), time });
  }
  return start === "list" ? text.charAt(0).toUpperCase() + text.slice(1) : text;
}

/** What a phone brings, as the second line of the phones list. */
export function bringsKey(phone: PhoneSources) {
  const calendar = phone.calendar !== null;
  const health = phone.health !== null;
  if (calendar && health) return "phones.brings.both" as const;
  if (calendar) return "phones.brings.calendar" as const;
  if (health) return "phones.brings.health" as const;
  return null;
}

/** "{name} · an update is ready for it" or "{name} needs an update before it can connect". */
export function updateLine(phone: PhoneSources): string | null {
  const update = phone.app?.update;
  if (!update) return null;
  return t(update === "ready" ? "phones.update.ready" : "phones.update.needed", {
    name: phone.name,
  });
}

/** "Android app · Nocturne 1.2", "iPhone app · …" or "In the browser"; nothing when not known. */
export function phoneKind(phone: PhoneSources): string | null {
  if (phone.inBrowser) return t("phones.kind.browser");
  const app = phone.app;
  if (!app) return null;
  return t(app.platform === "ios" ? "phones.kind.ios" : "phones.kind.android", {
    version: app.version,
  });
}
