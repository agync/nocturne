import ask from "./en-GB/ask.json";
import backup from "./en-GB/backup.json";
import common from "./en-GB/common.json";
import feelings from "./en-GB/feelings.json";
import health from "./en-GB/health.json";
import journal from "./en-GB/journal.json";
import journalCheckin from "./en-GB/journal-checkin.json";
import journalDay from "./en-GB/journal-day.json";
import journalLookback from "./en-GB/journal-lookback.json";
import journalQuestion from "./en-GB/journal-question.json";
import memories from "./en-GB/memories.json";
import moving from "./en-GB/moving.json";
import page from "./en-GB/page.json";
import phone from "./en-GB/phone.json";
import phoneAlone from "./en-GB/phone-alone.json";
import phoneApp from "./en-GB/phone-app.json";
import phoneBackup from "./en-GB/phone-backup.json";
import phoneHealth from "./en-GB/phone-health.json";
import phoneMind from "./en-GB/phone-mind.json";
import phoneNative from "./en-GB/phone-native.json";
import phones from "./en-GB/phones.json";

/**
 * The files of `en-GB/` the phone app draws on. The phone build swaps this in for `deck.ts`
 * (see `vite.phone.config.ts`), so the desktop-only files never ship to a phone. `phone-deck.test.ts`
 * fails when the phone's code names an id that is not here.
 */
export const enGB = {
  ...common,
  ...page,
  ...ask,
  ...memories,
  ...backup,
  ...phone,
  ...phoneNative,
  ...phoneApp,
  ...phoneHealth,
  ...phoneMind,
  ...phoneAlone,
  ...phoneBackup,
  ...moving,
  ...phones,
  ...health,
  ...journal,
  ...journalLookback,
  ...journalCheckin,
  ...journalDay,
  ...journalQuestion,
  ...feelings,
};
