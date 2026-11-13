import { defineConfig } from "vitest/config";

// The live cross-check between the Rust test phone and the TypeScript phone client. It runs
// against programs built from `src-tauri/tests`, so it is not part of `npm test`; see
// scripts/away-integration.sh.
export default defineConfig({
  test: {
    environment: "node",
    globals: true,
    include: ["interop/**/*.test.ts"],
    testTimeout: 60_000,
    hookTimeout: 60_000,
    fileParallelism: false,
  },
});
