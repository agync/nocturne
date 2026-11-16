import { hashHref } from "../../app/query";

/** Where the reader was opened from, which decides where Back goes. */
export type GuideFrom = "ask" | "settings";

/** `#/guide?page=…&section=…&from=…`: the reader at a section, or at the guide's first page. */
export function guideHref(
  from: GuideFrom,
  at: { page: string; section?: string } | null = null,
): string {
  return hashHref("/guide", { page: at?.page, section: at?.section, from });
}
