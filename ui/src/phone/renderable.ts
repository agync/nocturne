// What an original may be drawn as. A blob URL shares the phone app's origin, which holds the
// keys, so only the types the allowlist names are ever shown, and only as what they are:
// pictures, sound, PDFs and text. Anything that could run script (HTML, SVG, XML) is never
// embedded or navigated to.

/** `download` is a type that is kept and offered as a file but not drawn (HEIC, which most browsers cannot). */
export type Rendering = "image" | "audio" | "pdf" | "download";

const IMAGES = new Set(["image/jpeg", "image/png", "image/webp"]);
const AUDIO = new Set(["audio/webm", "audio/mp4", "audio/ogg", "audio/mpeg", "audio/wav"]);

export function renderingFor(mime: string): Rendering | null {
  if (IMAGES.has(mime)) return "image";
  if (mime === "image/heic") return "download";
  if (AUDIO.has(mime)) return "audio";
  if (mime === "application/pdf") return "pdf";
  return null;
}
