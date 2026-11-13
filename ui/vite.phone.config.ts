import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// The phone app: a second entry, built into dist/phone beside the desktop interface and served
// by the computer to paired phones. Nothing is inlined, so its policy needs no inline script or
// style. Tests use the scripted phone engine.
export default defineConfig(({ mode }) => {
  const engine =
    mode === "test" || process.env.VITE_ENGINE === "phone-mock" ? "phone-mock" : "phone";
  return {
    root: "phone",
    base: "/",
    plugins: [react()],
    define: {
      "import.meta.env.VITE_ENGINE": JSON.stringify(engine),
    },
    build: {
      outDir: "../dist/phone",
      emptyOutDir: true,
      assetsInlineLimit: 0,
      target: "es2022",
      sourcemap: false,
    },
    server: { port: 5174, strictPort: true },
  };
});
