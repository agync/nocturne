/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_ENGINE: "mock" | "tauri";
  readonly VITE_MOCK_NOW: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}

declare module "*.module.css" {
  const classes: Record<string, string>;
  export default classes;
}
