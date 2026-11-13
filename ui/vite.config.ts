import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// The production policy, also sent by `vite preview` so tests catch what the real app would block.
const CSP = [
  "default-src 'self'",
  "img-src 'self' asset: http://asset.localhost blob: data:",
  "media-src 'self' asset: http://asset.localhost blob:",
  "style-src 'self' 'unsafe-inline'",
  "font-src 'self'",
  "connect-src 'self' ipc: http://ipc.localhost",
].join("; ");

export default defineConfig(({ mode }) => {
  const engine =
    process.env.VITE_ENGINE ?? (mode === "production" || mode === "tauri" ? "tauri" : "mock");
  return {
    plugins: [react()],
    define: {
      "import.meta.env.VITE_ENGINE": JSON.stringify(engine),
      "import.meta.env.VITE_MOCK_NOW": JSON.stringify(process.env.VITE_MOCK_NOW ?? ""),
    },
    build: { assetsInlineLimit: 0, target: "es2022", sourcemap: false },
    preview: { headers: { "Content-Security-Policy": CSP } },
    server: { port: 5173, strictPort: true },
    test: {
      environment: "jsdom",
      globals: true,
      globalSetup: ["./src/test/global-setup.ts"],
      setupFiles: ["./src/test/setup.ts"],
      include: ["src/**/*.test.{ts,tsx}"],
      testTimeout: 30_000,
      hookTimeout: 30_000,
      css: { modules: { classNameStrategy: "non-scoped" } },
    },
  };
});
