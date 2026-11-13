/**
 * Files a person chose in the browser, held until the engine client sends them. The interface
 * passes "paths" around; in a browser a path is the key of one of these.
 */
const chosen = new Map<string, File>();
let counter = 0;

export function rememberFile(file: File): string {
  counter += 1;
  const key = `upload:${counter}:${file.name}`;
  chosen.set(key, file);
  return key;
}

/** The file for a key, once: a second ask finds nothing. */
export function takeFile(key: string): File | undefined {
  const file = chosen.get(key);
  chosen.delete(key);
  return file;
}

const BY_EXTENSION: Record<string, string> = {
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  png: "image/png",
  heic: "image/heic",
  webp: "image/webp",
  gif: "image/gif",
  m4a: "audio/mp4",
  mp3: "audio/mpeg",
  wav: "audio/wav",
  ogg: "audio/ogg",
  opus: "audio/opus",
  webm: "audio/webm",
  flac: "audio/flac",
  pdf: "application/pdf",
  txt: "text/plain",
  md: "text/markdown",
  docx: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
};

/** The type the browser reports, or the one the name implies. */
export function mimeOf(file: File): string {
  if (file.type) return file.type;
  const ext = file.name.split(".").pop()?.toLowerCase() ?? "";
  return BY_EXTENSION[ext] ?? "application/octet-stream";
}
