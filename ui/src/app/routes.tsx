import { type ComponentType, type LazyExoticComponent, lazy } from "react";
import type { CopyId } from "../copy/t";
import { pathOf } from "./query";

export interface AppRoute {
  /** wouter pattern, without the leading `#`. */
  path: string;
  folder: string;
  title: CopyId;
  /** False for screens that draw their own window: quick capture, setup, print. */
  shell: boolean;
  Component: LazyExoticComponent<ComponentType>;
}

const today = lazy(() => import("../screens/today"));
const journal = lazy(() => import("../screens/journal"));
const checkin = lazy(() => import("../screens/checkin"));
const lookback = lazy(() => import("../screens/lookback"));
const ask = lazy(() => import("../screens/ask"));
const needsYou = lazy(() => import("../screens/needs-you"));
const recent = lazy(() => import("../screens/recent"));
const capture = lazy(() => import("../screens/capture"));
const memories = lazy(() => import("../screens/memories"));
const fading = lazy(() => import("../screens/fading"));
const pastPages = lazy(() => import("../screens/past-pages"));
const setup = lazy(() => import("../screens/setup"));
const privacy = lazy(() => import("../screens/privacy"));
const backup = lazy(() => import("../screens/backup"));
const restore = lazy(() => import("../screens/restore"));
const settings = lazy(() => import("../screens/settings"));
const editions = lazy(() => import("../screens/editions"));
const away = lazy(() => import("../screens/away"));
const online = lazy(() => import("../screens/online"));
const models = lazy(() => import("../screens/models"));
const guide = lazy(() => import("../screens/guide"));

/** The route table is fixed: later lanes replace screen folders, never this list. */
export const routes: readonly AppRoute[] = [
  { path: "/today", folder: "today", title: "screen.today.title", shell: true, Component: today },
  {
    path: "/pages/:date",
    folder: "today",
    title: "screen.page.title",
    shell: true,
    Component: today,
  },
  {
    path: "/journal",
    folder: "journal",
    title: "screen.journal.title",
    shell: true,
    Component: journal,
  },
  {
    path: "/journal/check-in",
    folder: "checkin",
    title: "screen.checkin.title",
    shell: true,
    Component: checkin,
  },
  {
    path: "/journal/look-back/:kind/:start",
    folder: "lookback",
    title: "screen.lookback.title",
    shell: true,
    Component: lookback,
  },
  {
    path: "/journal/:day",
    folder: "journal",
    title: "screen.journal.title",
    shell: true,
    Component: journal,
  },
  { path: "/ask", folder: "ask", title: "screen.ask.title", shell: true, Component: ask },
  {
    path: "/needs-you",
    folder: "needs-you",
    title: "screen.needsYou.title",
    shell: true,
    Component: needsYou,
  },
  {
    path: "/recent",
    folder: "recent",
    title: "screen.recent.title",
    shell: true,
    Component: recent,
  },
  {
    path: "/quick-capture",
    folder: "capture",
    title: "screen.capture.title",
    shell: false,
    Component: capture,
  },
  {
    path: "/memories",
    folder: "memories",
    title: "screen.memories.title",
    shell: true,
    Component: memories,
  },
  {
    path: "/memories/:entityId",
    folder: "memories",
    title: "screen.entity.title",
    shell: true,
    Component: memories,
  },
  {
    path: "/items/:itemId",
    folder: "memories",
    title: "screen.item.title",
    shell: true,
    Component: memories,
  },
  {
    path: "/fading",
    folder: "fading",
    title: "screen.fading.title",
    shell: true,
    Component: fading,
  },
  {
    path: "/pages",
    folder: "past-pages",
    title: "screen.pages.title",
    shell: true,
    Component: pastPages,
  },
  { path: "/setup", folder: "setup", title: "screen.setup.title", shell: false, Component: setup },
  {
    path: "/privacy",
    folder: "privacy",
    title: "screen.privacy.title",
    shell: true,
    Component: privacy,
  },
  {
    path: "/privacy/backup",
    folder: "backup",
    title: "screen.backup.title",
    shell: true,
    Component: backup,
  },
  {
    path: "/privacy/restore",
    folder: "restore",
    title: "screen.restore.title",
    shell: true,
    Component: restore,
  },
  {
    path: "/settings",
    folder: "settings",
    title: "screen.settings.title",
    shell: true,
    Component: settings,
  },
  {
    path: "/settings/advanced",
    folder: "settings",
    title: "screen.advanced.title",
    shell: true,
    Component: settings,
  },
  {
    path: "/settings/away",
    folder: "away",
    title: "screen.away.title",
    shell: true,
    Component: away,
  },
  {
    path: "/settings/away/setup",
    folder: "away",
    title: "screen.away.setup.title",
    shell: true,
    Component: away,
  },
  {
    path: "/settings/away/add",
    folder: "away",
    title: "screen.away.add.title",
    shell: true,
    Component: away,
  },
  {
    path: "/settings/away/phones",
    folder: "away",
    title: "screen.away.phones.title",
    shell: true,
    Component: away,
  },
  {
    path: "/settings/away/activity",
    folder: "away",
    title: "screen.away.activity.title",
    shell: true,
    Component: away,
  },
  {
    path: "/settings/online",
    folder: "online",
    title: "screen.online.title",
    shell: true,
    Component: online,
  },
  {
    path: "/settings/online/setup",
    folder: "online",
    title: "screen.online.setup.title",
    shell: true,
    Component: online,
  },
  {
    path: "/settings/advanced/online",
    folder: "online",
    title: "screen.advanced.online.title",
    shell: true,
    Component: online,
  },
  {
    path: "/settings/online/sent",
    folder: "online",
    title: "screen.online.sent.title",
    shell: true,
    Component: online,
  },
  {
    path: "/settings/models",
    folder: "models",
    title: "screen.models.title",
    shell: true,
    Component: models,
  },
  {
    path: "/settings/models/answering",
    folder: "models",
    title: "screen.models.answering.title",
    shell: true,
    Component: models,
  },
  {
    path: "/settings/models/recordings",
    folder: "models",
    title: "screen.models.recordings.title",
    shell: true,
    Component: models,
  },
  {
    path: "/settings/models/aloud",
    folder: "models",
    title: "screen.models.aloud.title",
    shell: true,
    Component: models,
  },
  {
    path: "/settings/models/finding",
    folder: "models",
    title: "screen.models.finding.title",
    shell: true,
    Component: models,
  },
  {
    path: "/guide",
    folder: "guide",
    title: "screen.guide.title",
    shell: true,
    Component: guide,
  },
  {
    path: "/listen/:date",
    folder: "editions",
    title: "screen.listen.title",
    shell: true,
    Component: editions,
  },
  {
    path: "/print/:date",
    folder: "editions",
    title: "screen.print.title",
    shell: false,
    Component: editions,
  },
];

function matches(pattern: string, location: string): boolean {
  const re = new RegExp(`^${pattern.replace(/:[^/]+/g, "[^/]+")}/?$`);
  return re.test(location);
}

export function findRoute(location: string): AppRoute | undefined {
  const path = pathOf(location);
  return routes.find((r) => matches(r.path, path));
}
