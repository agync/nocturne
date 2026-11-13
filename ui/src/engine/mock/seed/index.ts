import type {
  EntityView,
  ItemView,
  MemoryView,
  PageView,
  SettingsView,
  SetupView,
} from "../../bindings";
import { captureSeed } from "./capture";
import { makeBase, makeReceipt, type SeedContext } from "./common";
import { memoriesSeed } from "./memories";
import { pagesSeed } from "./pages";
import { settingsSeed } from "./settings";
import { setupSeed } from "./setup";

export { DEFAULT_NOW, SAFETY_PREDICATES } from "./common";

export interface SeedData {
  items: ItemView[];
  memories: MemoryView[];
  entities: EntityView[];
  pages: PageView[];
  settings: SettingsView;
  setup: SetupView;
}

/**
 * Maya's two weeks, built relative to `now`. Each area has its own file in this folder, so the
 * lanes that own a screen can change its data without touching anyone else's.
 */
export function buildSeed(now: Date): SeedData {
  const base = makeBase(now);
  const items = captureSeed(base);
  const ctx: SeedContext = { ...base, receipt: makeReceipt(items) };
  const { memories, entities } = memoriesSeed(ctx);
  const pages = pagesSeed(ctx, memories);
  return { items, memories, entities, pages, settings: settingsSeed(), setup: setupSeed() };
}
