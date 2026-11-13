import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import react from "@vitejs/plugin-react";
import { defineConfig, type Plugin } from "vite";

const here = (path: string) => fileURLToPath(new URL(path, import.meta.url));
const { version } = JSON.parse(readFileSync(here("./package.json"), "utf8")) as { version: string };

/**
 * Gives the service worker the files of this build to keep. The worker itself is built as a
 * separate entry named `sw.js` and holds a placeholder that is filled in here.
 */
function shellFiles(): Plugin {
  return {
    name: "nocturne-phone-shell",
    generateBundle(_options, bundle) {
      const files = Object.keys(bundle).filter((f) => f !== "sw.js" && !f.endsWith(".map"));
      const id = createHash("sha256").update(files.sort().join("\n")).digest("hex").slice(0, 12);
      const sw = bundle["sw.js"];
      if (sw?.type !== "chunk") return;
      const json = JSON.stringify({ version: id, files });
      sw.code = sw.code.replace("__SHELL_JSON__", () =>
        json.replace(/\\/g, "\\\\").replace(/"/g, '\\"'),
      );
    },
  };
}

// The phone app: a second entry, built into dist/phone beside the desktop interface and served
// by the computer to paired phones. Nothing is inlined, so its policy needs no inline script or
// style. Tests use the scripted phone engine.
export default defineConfig(({ mode }) => {
  const engine =
    mode === "test" || process.env.VITE_ENGINE === "phone-mock" ? "phone-mock" : "phone";
  return {
    root: "phone",
    base: "/",
    plugins: [react(), shellFiles()],
    define: {
      "import.meta.env.VITE_ENGINE": JSON.stringify(engine),
      __APP_VERSION__: JSON.stringify(version),
    },
    build: {
      outDir: process.env.PHONE_OUT_DIR ?? "../dist/phone",
      emptyOutDir: true,
      assetsInlineLimit: 0,
      target: "es2022",
      sourcemap: false,
      rollupOptions: {
        input: { index: here("./phone/index.html"), sw: here("./src/phone/sw.ts") },
        output: {
          entryFileNames: (chunk) => (chunk.name === "sw" ? "sw.js" : "assets/[name]-[hash].js"),
        },
      },
    },
    server: { port: 5174, strictPort: true },
  };
});
