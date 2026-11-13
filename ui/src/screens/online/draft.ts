import type { OnlineLimit, OnlineSettings } from "../../engine/bindings";

/** What the person has chosen so far in the setup, before anything is turned on. */
export type Draft = OnlineSettings;

/** The service as the check found it: its host and the name to use. */
export interface Provider {
  host: string;
  model: string;
}

export interface StepProps {
  draft: Draft;
  provider: Provider;
  costReported: boolean;
  phonesAvailable: boolean;
  setDraft: (next: Draft) => void;
  onBack: () => void;
  onNext: () => void;
}

export const withLimit = (draft: Draft, limit: OnlineLimit): Draft => ({ ...draft, limit });
