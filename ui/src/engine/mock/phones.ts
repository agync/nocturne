import { formatLongDay, formatTime12, localDay } from "../../copy/format";
import type {
  CalendarDetail,
  CameInRow,
  DayPart,
  HealthSeries,
  ItemView,
  JournalDay,
  MemoryView,
  PageView,
  PhoneDeletePreview,
  PhoneDeleteScope,
  PhonePageOptions,
  PhoneSource,
  PhoneSources,
  PhoneSourcesView,
  ReadingKind,
  ReadingRow,
  RowKind,
  TimelineRow,
} from "../bindings";
import { type EngineClient, EngineError } from "../client";
import type { MockEngine } from "./engine";
import { buildDemo, type Demo } from "./phonesDemo";

/** The mock's phones that bring something. Empty: no app phone is paired. */
export interface PhonesFixtures {
  phones: PhoneSources[];
  cameIn: Record<string, CameInRow[]>;
  series: Partial<Record<ReadingKind, HealthSeries>>;
  readings: Partial<Record<ReadingKind, ReadingRow[]>>;
}

export const emptyPhones = (): PhonesFixtures => ({
  phones: [],
  cameIn: {},
  series: {},
  readings: {},
});

const clone = <T>(value: T): T => structuredClone(value);

/** From your phones and the health pages, over fixtures. */
export class PhonesSim {
  private page: PhonePageOptions = { calendarToday: true, sleepLastNight: true };
  private noticing = true;
  private readonly tokens = new Map<string, PhoneDeleteScope>();
  private counter = 0;

  constructor(readonly fixtures: PhonesFixtures = emptyPhones()) {}

  view(): PhoneSourcesView {
    return {
      phones: structuredClone(this.fixtures.phones),
      page: { ...this.page },
      noticing: this.noticing,
    };
  }

  pageOptions(): PhonePageOptions {
    return { ...this.page };
  }

  private phone(deviceId: string): PhoneSources {
    const phone = this.fixtures.phones.find((p) => p.deviceId === deviceId);
    if (!phone) throw new EngineError("notFound", `no phone ${deviceId}`);
    return phone;
  }

  /** Whether a phone is taking a source right now. */
  taking(source: PhoneSource): boolean {
    return this.fixtures.phones.some((p) => p[source]?.taking);
  }

  setSource(deviceId: string, source: PhoneSource, taking: boolean): PhoneSourcesView {
    const state = this.phone(deviceId)[source];
    if (!state) throw new EngineError("invalid", `${deviceId} does not bring ${source}`);
    state.taking = taking;
    return this.view();
  }

  setPageOptions(options: PhonePageOptions): PhoneSourcesView {
    this.page = { ...options };
    return this.view();
  }

  setNoticing(on: boolean): void {
    this.noticing = on;
  }

  /** Newest first, the fixtures' own order within one send; the cursor is the time and the item. */
  cameIn(
    deviceId: string,
    source: PhoneSource,
    before: string | null,
    limit: number,
    beforeId: string | null = null,
  ): CameInRow[] {
    this.phone(deviceId);
    const rows = [...(this.fixtures.cameIn[`${deviceId}/${source}`] ?? [])].sort((a, b) =>
      b.cameInAt.localeCompare(a.cameInAt),
    );
    const at = beforeId === null ? -1 : rows.findIndex((r) => r.itemId === beforeId);
    const rest =
      before === null
        ? rows
        : at >= 0
          ? rows.slice(at + 1)
          : rows.filter((r) => r.cameInAt < before);
    return clone(rest.slice(0, limit));
  }

  series(kind: ReadingKind, from: string, to: string): HealthSeries {
    const s = this.fixtures.series[kind];
    const points = (s?.points ?? []).filter((p) => p.day >= from && p.day <= to);
    return clone({
      kind,
      from,
      to,
      points,
      usual: s?.usual ?? null,
      journalDays: s?.journalDays ?? [],
      ...(s?.pressureUsual ? { pressureUsual: s.pressureUsual } : {}),
      ...(s?.usualBed ? { usualBed: s.usualBed } : {}),
      ...(s?.usualWake ? { usualWake: s.usualWake } : {}),
    });
  }

  readings(kind: ReadingKind, from: string, to: string): ReadingRow[] {
    return clone(
      (this.fixtures.readings[kind] ?? [])
        .filter((r) => r.day >= from && r.day <= to)
        .sort((a, b) => b.day.localeCompare(a.day) || b.start.localeCompare(a.start)),
    );
  }

  private matching(scope: PhoneDeleteScope): { rows: ReadingRow[]; kinds: ReadingKind[] } {
    const kinds = (scope.kinds ?? (Object.keys(this.fixtures.readings) as ReadingKind[])).filter(
      (k) => this.fixtures.readings[k],
    );
    const rows = kinds.flatMap((k) =>
      (this.fixtures.readings[k] ?? []).filter(
        (r) => scope.before === null || r.day < scope.before,
      ),
    );
    return { rows, kinds };
  }

  previewDelete(scope: PhoneDeleteScope): PhoneDeletePreview {
    this.phone(scope.deviceId);
    if (scope.itemId) {
      this.counter += 1;
      const token = `phones-delete-${this.counter}`;
      this.tokens.set(token, clone(scope));
      return {
        token,
        items: 1,
        versions: 0,
        memories: 0,
        pageLines: 0,
        recapLines: 0,
        noticing: 0,
        sharedMemories: 0,
      };
    }
    this.counter += 1;
    const token = `phones-delete-${this.counter}`;
    this.tokens.set(token, clone(scope));
    if (scope.source === "calendar") {
      const rows = this.fixtures.cameIn[`${scope.deviceId}/calendar`] ?? [];
      const known = rows.length === 0 ? 0 : 41;
      return {
        token,
        items: known,
        versions: known === 0 ? 0 : 9,
        memories: known === 0 ? 0 : known,
        pageLines: known === 0 ? 0 : 6,
        recapLines: 0,
        noticing: 0,
        sharedMemories: known === 0 ? 0 : 2,
      };
    }
    const { rows } = this.matching(scope);
    return {
      token,
      items: rows.length,
      versions: 0,
      memories: 0,
      pageLines: 0,
      recapLines: rows.length === 0 ? 0 : 3,
      noticing: rows.length === 0 ? 0 : 1,
    };
  }

  delete(token: string, alsoStop = false): void {
    const scope = this.tokens.get(token);
    if (!scope) throw new EngineError("tokenExpired", "preview again");
    this.tokens.delete(token);
    if (scope.itemId) {
      for (const kind of Object.keys(this.fixtures.readings) as ReadingKind[]) {
        this.fixtures.readings[kind] = (this.fixtures.readings[kind] ?? []).filter(
          (r) => r.itemId !== scope.itemId,
        );
        const series = this.fixtures.series[kind];
        if (series) series.points = series.points.filter((p) => p.itemId !== scope.itemId);
      }
    } else if (scope.source === "calendar") {
      this.fixtures.cameIn[`${scope.deviceId}/calendar`] = [];
      this.removedCalendar = true;
    } else {
      const { kinds } = this.matching(scope);
      for (const kind of kinds) {
        const keep = (day: string) => scope.before !== null && day >= scope.before;
        this.fixtures.readings[kind] = (this.fixtures.readings[kind] ?? []).filter((r) =>
          keep(r.day),
        );
        const series = this.fixtures.series[kind];
        if (series) series.points = series.points.filter((p) => keep(p.day));
      }
      this.removedHealth = scope.kinds === null && scope.before === null;
    }
    if (alsoStop) {
      const state = this.phone(scope.deviceId)[scope.source];
      if (state) state.taking = false;
    }
  }

  /** Set once the whole calendar or all of health is deleted, so the page and journal let go of it. */
  removedCalendar = false;
  removedHealth = false;

  /** Forgets everything a phone brought, as removing it with "Also delete what it brought" does. */
  deleteAllFrom(deviceId: string): void {
    const phone = this.fixtures.phones.find((p) => p.deviceId === deviceId);
    if (!phone) return;
    this.fixtures.cameIn[`${deviceId}/calendar`] = [];
    this.removedCalendar = true;
    this.removedHealth = true;
    this.fixtures.readings = {};
    this.fixtures.series = {};
  }

  forgetPhone(deviceId: string): void {
    this.fixtures.phones = this.fixtures.phones.filter((p) => p.deviceId !== deviceId);
  }
}

const partOf = (instant: string): DayPart => {
  const hour = new Date(instant).getHours();
  if (hour < 12) return "morning";
  if (hour < 18) return "afternoon";
  return "evening";
};

/**
 * The mock engine with two phones, Sam's with the app and Ana's in the browser, bringing the
 * calendar and health of the mockups. Everything is relative to the mock's clock. The engine's
 * own handlers answer for the phone commands and for what they add to the page, the journal, an
 * item and the list of phones, so nothing in the engine file changes.
 */
export function withPhones(
  engine: MockEngine,
  now: Date | string,
  options: { empty?: boolean } = {},
): MockEngine & { demo: Demo; sim: PhonesSim } {
  const demo = buildDemo(new Date(now));
  const sim = new PhonesSim(options.empty ? emptyPhones() : demo.fixtures);
  const today = localDay(new Date(now));
  const base = {
    getPage: engine.getPage.bind(engine),
    getJournalDay: engine.getJournalDay.bind(engine),
    getItem: engine.getItem.bind(engine),
    listMemories: engine.listMemories.bind(engine),
    getRemote: engine.getRemote.bind(engine),
    removeDevice: engine.removeDevice.bind(engine),
  };

  const calendarLive = () => sim.taking("calendar") && !sim.removedCalendar;
  const healthLive = () => sim.taking("health") && !sim.removedHealth;
  const calendarItem = (id: string): ItemView | undefined => {
    const found = demo.items.get(id);
    if (!found?.calendar) return undefined;
    return sim.removedCalendar ? undefined : found;
  };
  const readingItem = (id: string): ItemView | undefined => {
    const found = demo.items.get(id);
    return found && !found.calendar && !sim.removedHealth ? found : undefined;
  };

  const target = engine as MockEngine & { demo: Demo; sim: PhonesSim };
  target.demo = demo;
  target.sim = sim;
  target.getPhoneSources = async () => sim.view();
  target.setPhoneSource = async (deviceId, source, taking) =>
    sim.setSource(deviceId, source, taking);
  target.setPhonePageOptions = async (opts) => sim.setPageOptions(opts);
  target.setNoticing = async (on) => sim.setNoticing(on);
  // The mock engine file declares four arguments; the client interface has the cursor too.
  const client: EngineClient = target;
  client.listCameIn = async (deviceId, source, before, limit, beforeId) =>
    sim.cameIn(deviceId, source, before, limit, beforeId ?? null);
  target.getHealthSeries = async (kind, from, to) => sim.series(kind, from, to);
  target.listReadings = async (kind, from, to) => sim.readings(kind, from, to);
  target.previewPhoneDelete = async (scope) => sim.previewDelete(scope);
  target.deletePhoneData = async (token, alsoStop) => sim.delete(token, alsoStop);

  target.getRemote = async () => {
    const view = await base.getRemote();
    const names = new Map(sim.fixtures.phones.map((p) => [p.deviceId, p.name]));
    return {
      ...view,
      devices: view.devices.map((d) => ({ ...d, name: names.get(d.id) ?? d.name })),
    };
  };
  target.removeDevice = async (id, alsoDeleteBrought) => {
    await base.removeDevice(id, alsoDeleteBrought);
    if (alsoDeleteBrought) sim.deleteAllFrom(id);
    sim.forgetPhone(id);
  };

  target.getPage = async (date) => {
    const page: PageView = await base.getPage(date);
    if (page.date !== today) return page;
    const out: PageView = { ...page };
    const view = sim.pageOptions();
    if (view.calendarToday && calendarLive()) {
      const lines = demo.calendarToday.filter((l) => calendarItem(l.itemId));
      if (lines.length > 0) {
        out.calendarToday = lines.slice(0, 5);
        out.moreCalendarToday = Math.max(0, lines.length - 5);
      }
    }
    if (view.sleepLastNight && healthLive()) out.lastNight = clone(demo.lastNight);
    return out;
  };

  target.getItem = async (id) => {
    const found = calendarItem(id) ?? readingItem(id);
    if (found) return clone(found);
    return base.getItem(id);
  };

  /** One plain memory per calendar entry, made from its fields, as the engine makes them. */
  const memoryOf = (item: ItemView): MemoryView => ({
    id: `mem-${item.id}`,
    sentence: `${item.title}${item.calendar?.place ? ` at ${item.calendar.place}` : ""}, ${formatLongDay(item.calendar?.start ?? item.keptAt)} at ${formatTime12(item.calendar?.start ?? item.keptAt)}.`,
    subject: item.title,
    entityId: null,
    predicate: "scheduled",
    object: item.title,
    certainty: "sure",
    status: "active",
    validFrom: item.calendar?.start ?? null,
    validTo: null,
    due: item.calendar?.start ?? null,
    learnedAt: item.keptAt,
    lastUsed: null,
    letGoAt: null,
    keepForever: false,
    private: false,
    was: null,
    movedFrom: null,
    receipts: [
      {
        itemId: item.id,
        kind: "calendar",
        source: "phone",
        label: item.title,
        keptAt: item.keptAt,
        durationMs: null,
        span: null,
        unconfirmed: false,
        fromDevice: item.fromDevice,
      },
    ],
  });
  target.listMemories = async (filter) => {
    const item = filter.itemId ? calendarItem(filter.itemId) : undefined;
    if (item && !item.calendar?.removed) return [memoryOf(item)];
    return base.listMemories(filter);
  };

  target.getJournalDay = async (day) => {
    const data: JournalDay = await base.getJournalDay(day);
    if (data.day !== today) return data;
    const out: JournalDay = { ...data, rows: [...data.rows], checkins: [...data.checkins] };
    const extra: TimelineRow[] = [];
    for (const entry of demo.timeline) {
      const live = entry.kind === "calendar" ? calendarLive() : healthLive();
      if (!live) continue;
      const item =
        entry.kind === "calendar" ? calendarItem(entry.item.id) : readingItem(entry.item.id);
      if (!item) continue;
      extra.push({
        kind: entry.kind as RowKind,
        part: partOf(entry.at),
        item: clone(item as ItemView),
        checkin: null,
        good: null,
        feeling: null,
        mentions: [],
        leftOut: false,
        moveTo: null,
      });
    }
    out.rows = [...out.rows, ...extra].sort(
      (a, b) => Date.parse(a.item.keptAt) - Date.parse(b.item.keptAt),
    );
    out.fromPhones = {
      calendar: calendarLive()
        ? clone(demo.brought.calendar).filter((i) => calendarItem(i.itemId))
        : [],
      health: healthLive() ? clone(demo.brought.health).filter((i) => readingItem(i.itemId)) : [],
    };
    if (healthLive()) {
      out.health = clone(demo.dayHealth);
      const first = [...out.checkins].sort((a, b) => a.keptAt.localeCompare(b.keptAt))[0];
      if (first) {
        const withSleep = {
          ...first,
          afterSleep: { minutes: demo.lastNight.asleepMin, itemId: demo.lastNight.itemId },
        };
        out.checkins = out.checkins.map((c) => (c.id === first.id ? withSleep : c));
        out.rows = out.rows.map((r) =>
          r.checkin?.id === first.id ? { ...r, checkin: withSleep } : r,
        );
      }
    }
    return out;
  };

  return target;
}

/** The calendar detail of an item the mock holds, for a screen test that needs it. */
export function calendarOf(item: ItemView): CalendarDetail | null {
  return item.calendar ?? null;
}
