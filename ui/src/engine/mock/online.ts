import type {
  CheckOutcome,
  OnlineAdvanced,
  OnlineForm,
  OnlineSend,
  OnlineSettings,
  OnlineSettingsPatch,
  OnlineView,
  PrivacyView,
} from "../bindings";
import { EngineError } from "../client";

const unavailable = () => new EngineError("unavailable", "Online answers is not available yet");

/** Online answers in the mock: off and never set up. The rest is not available yet. */
export class OnlineSim {
  view(): OnlineView {
    return {
      state: { type: "off" },
      setUp: false,
      host: null,
      baseUrl: null,
      model: null,
      phones: true,
      phonesAvailable: false,
      sendSensitive: false,
      limit: { kind: "questions", questions: 200, usd: null },
      questionsThisMonth: 0,
      spentThisMonthUsd: null,
      costReported: false,
      resetsOn: "",
      lastSentAt: null,
      onSince: null,
      lastOnAt: null,
      sentBytes: 0,
      keyStorage: null,
    };
  }

  privacy(): Pick<
    PrivacyView,
    "sentOnlineBytes" | "onlineQuestions" | "onlineOn" | "onlineHost" | "onlineLastOnAt"
  > {
    return {
      sentOnlineBytes: 0,
      onlineQuestions: 0,
      onlineOn: false,
      onlineHost: null,
      onlineLastOnAt: null,
    };
  }

  advanced(): OnlineAdvanced | null {
    return null;
  }

  async check(_form: OnlineForm): Promise<CheckOutcome> {
    throw unavailable();
  }

  async cancelCheck(): Promise<void> {
    throw unavailable();
  }

  async turnOn(_settings: OnlineSettings): Promise<OnlineView> {
    throw unavailable();
  }

  async turnOff(_forget: boolean): Promise<OnlineView> {
    throw unavailable();
  }

  async abandonSetup(): Promise<void> {
    throw unavailable();
  }

  async update(_patch: OnlineSettingsPatch): Promise<OnlineView> {
    throw unavailable();
  }

  async sends(_limit: number, _before: number | null): Promise<OnlineSend[]> {
    throw unavailable();
  }
}
