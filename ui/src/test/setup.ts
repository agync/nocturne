import "@testing-library/jest-dom/vitest";
import { cleanup, configure } from "@testing-library/react";
import { afterEach } from "vitest";

// Screens load on first use, which is slow while other suites share the machine. Loading them
// here, before a test starts, keeps that cost out of every wait.
await Promise.all([
  import("../screens/today"),
  import("../screens/journal"),
  import("../screens/checkin"),
  import("../screens/lookback"),
  import("../screens/ask"),
  import("../screens/needs-you"),
  import("../screens/recent"),
  import("../screens/capture"),
  import("../screens/memories"),
  import("../screens/fading"),
  import("../screens/past-pages"),
  import("../screens/setup"),
  import("../screens/privacy"),
  import("../screens/settings"),
  import("../screens/editions"),
]);

configure({ asyncUtilTimeout: 5_000 });

afterEach(() => {
  cleanup();
});
