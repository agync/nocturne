/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** `phone` and `phone-mock` are the phone app's (vite.phone.config.ts). */
  readonly VITE_ENGINE: "mock" | "tauri" | "http" | "phone" | "phone-mock";
  readonly VITE_MOCK_NOW: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}

declare module "*.module.css" {
  const classes: Record<string, string>;
  export default classes;
}
