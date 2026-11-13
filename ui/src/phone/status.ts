// What the computer's listener says over HTTP and over a WebSocket, by name, and the phone's own
// build. These are the listener's contract (src-tauri/src/remote/listener.rs); they move to
// protocol.ts as `HTTP_OFF`, `HTTP_ENDED`, `HTTP_POLL_BUSY`, `HTTP_REFUSED`, `HTTP_NOT_FOUND`,
// `WS_REFUSED`, `PHONE_BUILD` and `MIN_PHONE_BUILD` when that amendment lands on main.

/** Every keyed request while Away from home is off. */
export const HTTP_OFF = 503;
/** The session or pairing the request names has ended: open a new one. */
export const HTTP_ENDED = 410;
/** Another held poll for the same id is already waiting. */
export const HTTP_POLL_BUSY = 409;
/** The unsealed refusal. */
export const HTTP_REFUSED = 403;
/** Anything the listener does not serve. */
export const HTTP_NOT_FOUND = 404;
/** The unsealed refusal over a WebSocket: a close with this code and no reason. */
export const WS_REFUSED = 4403;

/** The build of this phone app. A computer that needs a newer one says so in its Hello. */
export const PHONE_BUILD = 1;

/** The smallest phone build the computer's Hello asks for, 0 when it asks for none. */
export function minPhoneBuild(hello: object): number {
  const n = (hello as { minPhoneBuild?: unknown }).minPhoneBuild;
  return typeof n === "number" && Number.isFinite(n) ? n : 0;
}
