// What the phone's browser must have for the sealed channel and the private key. A browser that
// lacks any of it gets "This browser is too old" and nothing else: no weaker fallback.

export interface Scope {
  isSecureContext?: boolean;
  crypto?: { subtle?: { generateKey?: unknown }; getRandomValues?: unknown };
  indexedDB?: unknown;
  fetch?: unknown;
  TextEncoder?: unknown;
  Blob?: unknown;
  URL?: unknown;
}

/** The names of what is missing, empty when the browser will do. */
export function missingFeatures(scope: Scope = globalThis as unknown as Scope): string[] {
  const missing: string[] = [];
  if (scope.isSecureContext !== true) missing.push("secure context");
  if (typeof scope.crypto?.subtle?.generateKey !== "function") missing.push("WebCrypto");
  if (typeof scope.crypto?.getRandomValues !== "function") missing.push("random numbers");
  if (!scope.indexedDB) missing.push("storage");
  if (typeof scope.fetch !== "function") missing.push("fetch");
  if (typeof scope.TextEncoder !== "function") missing.push("text encoding");
  if (typeof scope.Blob !== "function") missing.push("files");
  if (typeof scope.URL !== "function") missing.push("addresses");
  return missing;
}
