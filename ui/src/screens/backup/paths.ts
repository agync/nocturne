/** The last part of a path, whichever separator the system uses. */
export function fileNameOf(path: string): string {
  const at = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return at === -1 ? path : path.slice(at + 1);
}

/** The folder a file sits in, without a trailing separator. */
export function folderOf(path: string): string {
  const at = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  if (at <= 0) return at === 0 ? path.slice(0, 1) : "";
  return path.slice(0, at);
}
