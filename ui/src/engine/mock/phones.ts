import type {
  CameInRow,
  HealthSeries,
  PhoneDeletePreview,
  PhoneDeleteScope,
  PhonePageOptions,
  PhoneSource,
  PhoneSources,
  PhoneSourcesView,
  ReadingKind,
  ReadingRow,
} from "../bindings";
import { EngineError } from "../client";

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

/** From your phones and the health pages, over fixtures. */
export class PhonesSim {
  private page: PhonePageOptions = { calendarToday: true, sleepLastNight: true };
  private noticing = true;
  private readonly tokens = new Set<string>();
  private counter = 0;

  constructor(private readonly fixtures: PhonesFixtures = emptyPhones()) {}

  view(): PhoneSourcesView {
    return {
      phones: structuredClone(this.fixtures.phones),
      page: { ...this.page },
      noticing: this.noticing,
    };
  }

  private phone(deviceId: string): PhoneSources {
    const phone = this.fixtures.phones.find((p) => p.deviceId === deviceId);
    if (!phone) throw new EngineError("notFound", `no phone ${deviceId}`);
    return phone;
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

  cameIn(deviceId: string, source: PhoneSource, before: string | null, limit: number): CameInRow[] {
    this.phone(deviceId);
    return (this.fixtures.cameIn[`${deviceId}/${source}`] ?? [])
      .filter((row) => before === null || row.cameInAt < before)
      .slice(0, limit);
  }

  series(kind: ReadingKind, from: string, to: string): HealthSeries {
    const s = this.fixtures.series[kind];
    const points = (s?.points ?? []).filter((p) => p.day >= from && p.day <= to);
    return { kind, from, to, points, usual: s?.usual ?? null, journalDays: s?.journalDays ?? [] };
  }

  readings(kind: ReadingKind, from: string, to: string): ReadingRow[] {
    return (this.fixtures.readings[kind] ?? []).filter((r) => r.day >= from && r.day <= to);
  }

  previewDelete(scope: PhoneDeleteScope): PhoneDeletePreview {
    this.phone(scope.deviceId);
    this.counter += 1;
    const token = `phones-delete-${this.counter}`;
    this.tokens.add(token);
    return {
      token,
      items: 0,
      versions: 0,
      memories: 0,
      pageLines: 0,
      recapLines: 0,
      noticing: 0,
    };
  }

  delete(token: string): void {
    if (!this.tokens.delete(token)) throw new EngineError("tokenExpired", "preview again");
  }
}
