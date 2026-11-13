import "@testing-library/jest-dom/vitest";
import { cleanup, configure } from "@testing-library/react";
import { afterEach } from "vitest";

// A screen renders slowly while other suites share the machine; a wait that gives up early would
// fail on load, not on a defect. Passing tests never wait this long.
configure({ asyncUtilTimeout: 15_000 });

afterEach(() => {
  cleanup();
});
