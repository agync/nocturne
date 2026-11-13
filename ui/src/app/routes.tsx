import { type ComponentType, type LazyExoticComponent, lazy } from "react";
import type { CopyId } from "../copy/t";

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
const ask = lazy(() => import("../screens/ask"));
const needsYou = lazy(() => import("../screens/needs-you"));
const recent = lazy(() => import("../screens/recent"));
const capture = lazy(() => import("../screens/capture"));
const memories = lazy(() => import("../screens/memories"));
const fading = lazy(() => import("../screens/fading"));
const pastPages = lazy(() => import("../screens/past-pages"));
const setup = lazy(() => import("../screens/setup"));
const privacy = lazy(() => import("../screens/privacy"));
const settings = lazy(() => import("../screens/settings"));
const editions = lazy(() => import("../screens/editions"));

/** The route table is fixed: later lanes replace screen folders, never this list. */
export const routes: readonly AppRoute[] = [
  { path: "/today", folder: "today", title: "screen.today.title", shell: true, Component: today },
  {
    path: "/pages/:date",
    folder: "today",
    title: "screen.today.title",
    shell: true,
    Component: today,
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
  return routes.find((r) => matches(r.path, location));
}
