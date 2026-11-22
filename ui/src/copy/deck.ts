import advanced from "./en-GB/advanced.json";
import ask from "./en-GB/ask.json";
import away from "./en-GB/away.json";
import backup from "./en-GB/backup.json";
import capture from "./en-GB/capture.json";
import common from "./en-GB/common.json";
import editions from "./en-GB/editions.json";
import fading from "./en-GB/fading.json";
import feelings from "./en-GB/feelings.json";
import guide from "./en-GB/guide.json";
import health from "./en-GB/health.json";
import journal from "./en-GB/journal.json";
import journalCheckin from "./en-GB/journal-checkin.json";
import journalDay from "./en-GB/journal-day.json";
import journalDelete from "./en-GB/journal-delete.json";
import journalLookback from "./en-GB/journal-lookback.json";
import journalQuestion from "./en-GB/journal-question.json";
import memories from "./en-GB/memories.json";
import models from "./en-GB/models.json";
import needsYou from "./en-GB/needs-you.json";
import notice from "./en-GB/notice.json";
import online from "./en-GB/online.json";
import page from "./en-GB/page.json";
import phone from "./en-GB/phone.json";
import phoneAlone from "./en-GB/phone-alone.json";
import phoneApp from "./en-GB/phone-app.json";
import phoneHealth from "./en-GB/phone-health.json";
import phoneNative from "./en-GB/phone-native.json";
import phones from "./en-GB/phones.json";
import privacy from "./en-GB/privacy.json";
import recent from "./en-GB/recent.json";
import sent from "./en-GB/sent.json";
import settings from "./en-GB/settings.json";
import setup from "./en-GB/setup.json";
import shell from "./en-GB/shell.json";

/**
 * One file per area under `en-GB/`, so lanes that own different screens never edit the same
 * file. Every file is registered here once, so `keyof typeof enGB` is the checked set of ids.
 * A key that appears in two files fails `deck.test.ts`.
 */
export const enGB = {
  ...common,
  ...shell,
  ...page,
  ...capture,
  ...notice,
  ...ask,
  ...needsYou,
  ...recent,
  ...memories,
  ...fading,
  ...setup,
  ...backup,
  ...privacy,
  ...settings,
  ...advanced,
  ...editions,
  ...away,
  ...phone,
  ...phoneNative,
  ...phoneApp,
  ...phoneHealth,
  ...phoneAlone,
  ...phones,
  ...health,
  ...online,
  ...sent,
  ...models,
  ...guide,
  ...journal,
  ...journalLookback,
  ...journalCheckin,
  ...journalDay,
  ...journalDelete,
  ...journalQuestion,
  ...feelings,
};
