// Scanning the computer's code with the phone's own scanner. The camera is asked for the first
// time here, with the phone's own prompt; a refusal is told apart so the screen can say so.

import { invoke } from "@tauri-apps/api/core";

export type ScanResult =
  | { kind: "code"; text: string }
  | { kind: "cameraOff" }
  | { kind: "cancelled" };

type PermissionState = "granted" | "denied" | "prompt" | "prompt-with-rationale";

async function camera(cmd: "check_permissions" | "request_permissions"): Promise<PermissionState> {
  return invoke<{ camera: PermissionState }>(`plugin:barcode-scanner|${cmd}`).then(
    (p) => p.camera,
    () => "denied" as PermissionState,
  );
}

export async function scanCode(): Promise<ScanResult> {
  let state = await camera("check_permissions");
  if (state !== "granted") state = await camera("request_permissions");
  if (state !== "granted") return { kind: "cameraOff" };
  try {
    const got = await invoke<{ content: string }>("plugin:barcode-scanner|scan", {
      windowed: false,
      formats: ["QR_CODE"],
    });
    return { kind: "code", text: got.content };
  } catch {
    return { kind: "cancelled" };
  }
}

export function cancelScan(): void {
  void invoke("plugin:barcode-scanner|cancel").catch(() => undefined);
}

/** This app's page in the phone's settings, where the camera can be turned back on. */
export function openAppSettings(): Promise<void> {
  return invoke<void>("plugin:nocturne-system|open_app_settings").catch(() => undefined);
}

/** The phone's own name, where the platform gives it. */
export function phoneName(): Promise<string | null> {
  return invoke<string | null>("plugin:nocturne-system|device_name").catch(() => null);
}
