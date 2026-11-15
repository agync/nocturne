import advanced from "./en-GB/advanced.json";
import ask from "./en-GB/ask.json";
import away from "./en-GB/away.json";
import backup from "./en-GB/backup.json";
import capture from "./en-GB/capture.json";
import common from "./en-GB/common.json";
import editions from "./en-GB/editions.json";
import fading from "./en-GB/fading.json";
import memories from "./en-GB/memories.json";
import models from "./en-GB/models.json";
import needsYou from "./en-GB/needs-you.json";
import notice from "./en-GB/notice.json";
import online from "./en-GB/online.json";
import page from "./en-GB/page.json";
import phone from "./en-GB/phone.json";
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
  ...online,
  ...sent,
  ...models,
};
