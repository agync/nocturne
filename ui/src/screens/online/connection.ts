import type { Provider } from "./draft";

let passed: Provider | null = null;

/** The service whose check passed in Advanced details and has not been used or dropped yet. */
export const passedCheck = (): Provider | null => passed;

export const keepPassedCheck = (provider: Provider | null): void => {
  passed = provider;
};
