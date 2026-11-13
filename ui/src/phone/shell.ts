// Keeping the app's own files current. A computer that answers with a version other than this
// build's means the shell on the phone is stale: ask for the new one and reload, once.

const GUARD = "nocturne-shell-reloaded";

export async function refreshShellOnce(version: string): Promise<void> {
  try {
    // Only the version that was already tried is kept, so a shell that stays stale cannot loop.
    if (sessionStorage.getItem(GUARD) === version) return;
    sessionStorage.setItem(GUARD, version);
  } catch {
    return;
  }
  const worker = navigator.serviceWorker;
  if (!worker) return location.reload();
  const registration = await worker.getRegistration();
  if (!registration) return location.reload();
  worker.addEventListener("controllerchange", () => location.reload(), { once: true });
  await registration.update().catch(() => undefined);
  if (registration.waiting) registration.waiting.postMessage({ type: "skip-waiting" });
  else if (!registration.installing) location.reload();
}

export function registerWorker(): void {
  if (!("serviceWorker" in navigator)) return;
  const register = () => {
    navigator.serviceWorker.register("/sw.js", { scope: "/" }).catch(() => undefined);
  };
  if (document.readyState === "complete") register();
  else window.addEventListener("load", register, { once: true });
}
