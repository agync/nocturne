// Not a check: writes the data folder `make netcheck-remote` starts from, a store in which Away
// from home was left on (connection: Simple), made by the real app's own commands. Runs only
// when AWAY_NETCHECK_STORE names the folder to write.

import { rmSync } from "node:fs";
import { expect, test } from "vitest";
import { Driver } from "./driver";

const target = process.env.AWAY_NETCHECK_STORE;

test.skipIf(!target)("writes a store with Away from home left on", async () => {
  const d = await Driver.start();
  try {
    rmSync(`${target}/store`, { recursive: true, force: true });
    expect(await d.call("save_store", { to: target })).toBe(true);
  } finally {
    await d.stop();
  }
});
