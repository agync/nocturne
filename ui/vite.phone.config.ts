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

/**
 * `copy/t.ts` reads the whole desktop deck. The phone build reads `copy/phone-deck.ts` in its
 * place, so the phone ships only the copy files its screens use.
 */
function phoneDeck(): Plugin {
  const full = here("./src/copy/deck.ts");
  const phone = here("./src/copy/phone-deck.ts");
  return {
    name: "nocturne-phone-deck",
    enforce: "pre",
    async resolveId(source, importer, options) {
      if (!importer || !/(^|\/)deck$/.test(source)) return null;
      const resolved = await this.resolve(source, importer, { ...options, skipSelf: true });
      return resolved?.id === full ? phone : null;
    },
  };
}

// The phone app: a second entry, built into dist/phone beside the desktop interface and served
// by the computer to paired phones. Nothing is inlined, so its policy needs no inline script or
// style. Tests use the scripted phone engine. `--mode native` builds the same screens for the
// phone app into dist/phone-native: no service worker, since the files come with the app.
export default defineConfig(({ mode }) => {
  const engine =
    mode === "test" || process.env.VITE_ENGINE === "phone-mock" ? "phone-mock" : "phone";
  const native = mode === "native";
  const input: Record<string, string> = { index: here("./phone/index.html") };
  if (!native) input.sw = here("./src/phone/sw.ts");
  return {
    root: "phone",
    base: "/",
    plugins: native ? [phoneDeck(), react()] : [phoneDeck(), react(), shellFiles()],
    define: {
      "import.meta.env.VITE_ENGINE": JSON.stringify(engine),
      // The phone app's own screens run only in the phone app (`--mode native`) and the browser
      // tests' build of it; the app the computer serves to paired phones never ships them.
      "import.meta.env.VITE_PHONE_NATIVE": JSON.stringify(mode === "production" ? "0" : "1"),
      __APP_VERSION__: JSON.stringify(version),
    },
    build: {
      outDir: process.env.PHONE_OUT_DIR ?? (native ? "../dist/phone-native" : "../dist/phone"),
      emptyOutDir: true,
      assetsInlineLimit: 0,
      target: "es2022",
      sourcemap: false,
      rollupOptions: {
        input,
        output: {
          entryFileNames: (chunk) => (chunk.name === "sw" ? "sw.js" : "assets/[name]-[hash].js"),
        },
      },
    },
    server: { port: 5174, strictPort: true },
  };
});
