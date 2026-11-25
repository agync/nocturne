/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** `phone` and `phone-mock` are the phone app's (vite.phone.config.ts). */
  readonly VITE_ENGINE: "mock" | "tauri" | "http" | "phone" | "phone-mock";
  readonly VITE_MOCK_NOW: string;
  /** "0" in the app the computer serves to paired phones, which never runs the phone app's own screens. */
  readonly VITE_PHONE_NATIVE?: "0" | "1";
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}

declare module "*.module.css" {
  const classes: Record<string, string>;
  export default classes;
}
