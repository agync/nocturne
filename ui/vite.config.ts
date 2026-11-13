import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// The production policy, also sent by `vite preview` so tests catch what the real app would block.
// The preview with the mock engine also lets the page read the recordings it holds in memory.
const csp = (mock: boolean) =>
  [
    "default-src 'self'",
    "img-src 'self' asset: http://asset.localhost blob: data:",
    "media-src 'self' asset: http://asset.localhost blob:",
    "style-src 'self' 'unsafe-inline'",
    "font-src 'self'",
    `connect-src 'self' ipc: http://ipc.localhost${mock ? " blob:" : ""}`,
  ].join("; ");

export default defineConfig(({ mode }) => {
  // `preview-real` is the development build served by the preview server: the real engine over
  // HTTP, in its own output folder so it is never mistaken for the packaged interface.
  const preview = mode === "preview-real";
  const engine =
    process.env.VITE_ENGINE ??
    (preview ? "http" : mode === "production" || mode === "tauri" ? "tauri" : "mock");
  if (engine === "http" && !preview) {
    // An interface that talks to the preview server must never be packaged by accident.
    throw new Error("VITE_ENGINE=http is for `--mode preview-real` only");
  }
  return {
    plugins: [react()],
    define: {
      "import.meta.env.VITE_ENGINE": JSON.stringify(engine),
      "import.meta.env.VITE_MOCK_NOW": JSON.stringify(process.env.VITE_MOCK_NOW ?? ""),
    },
    build: {
      assetsInlineLimit: 0,
      target: "es2022",
      sourcemap: false,
      ...(preview ? { outDir: "dist-preview" } : {}),
    },
    preview: { headers: { "Content-Security-Policy": csp(engine === "mock") } },
    server: { port: 5173, strictPort: true },
    test: {
      environment: "jsdom",
      globals: true,
      globalSetup: ["./src/test/global-setup.ts"],
      setupFiles: ["./src/test/setup.ts"],
      include: ["src/**/*.test.{ts,tsx}"],
      css: { modules: { classNameStrategy: "non-scoped" } },
    },
  };
});
