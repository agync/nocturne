import type {
  GuideBlock,
  GuideCite,
  GuideInline,
  GuidePage,
  GuideView,
  HelpKind,
  HelpView,
} from "../bindings";
import seed from "./seed/guide.json";

/**
 * The Nocturne guide as the mock shows it: the bundled guide, written from the app's own copy by
 * a Rust test (`help::tests::the_mock_guide_is_the_bundled_guide`), and a simple match on the
 * ways people ask. The real matching is in `crates/engine/src/guide.rs`.
 */
const PAGES = seed.pages as GuidePage[];
const ASKED = seed.asked as Record<string, string[]>;

export function mockGuide(version: string): GuideView {
  return structuredClone({ version, pages: PAGES });
}

const words = (text: string): string[] =>
  text
    .toLowerCase()
    .replaceAll("’", "'")
    .split(/[^a-z0-9']+/)
    .filter((w) => w.length > 1);

const HOW_TO =
  /^(how (do|can|to|does)|can (i|it|nocturne)|does |where (do|can|has)|why |will (it|my))/i;

export const plain = (parts: GuideInline[]): string => parts.map((p) => p.text).join("");

function firstSentence(parts: GuideInline[]): GuideInline[] {
  const out: GuideInline[] = [];
  for (const part of parts) {
    if (part.type === "text" || part.type === "strong") {
      const end = part.text.search(/[.!?](\s|$)/);
      if (end >= 0) {
        out.push({ ...part, text: part.text.slice(0, end + 1) });
        return out;
      }
    }
    out.push(part);
  }
  return out;
}

function section(id: string) {
  const [pageSlug, sectionSlug] = id.replace("guide:", "").split("#");
  const page = PAGES.find((p) => p.slug === pageSlug);
  const found = page?.sections.find((s) => s.slug === sectionSlug);
  return page && found ? { page, section: found } : null;
}

function steps(blocks: GuideBlock[]): GuideInline[][] {
  const steps = blocks.find((b) => b.type === "steps");
  const list = blocks.find((b) => b.type === "list");
  const items = steps?.type === "steps" ? steps.items : list?.type === "list" ? list.items : null;
  if (items) return items.map(firstSentence);
  return blocks.flatMap((b) => (b.type === "paragraph" ? [firstSentence(b.parts)] : []));
}

export function cite(id: string): GuideCite | null {
  const found = section(id);
  if (!found) return null;
  const first = found.section.blocks[0];
  const quote =
    first?.type === "steps" || first?.type === "list"
      ? plain(first.items[0] ?? [])
      : first?.type === "paragraph"
        ? plain(firstSentence(first.parts))
        : found.section.heading;
  return {
    id,
    page: found.page.slug,
    pageTitle: found.page.title,
    section: found.section.slug,
    heading: found.section.heading,
    quote,
    steps: steps(found.section.blocks),
  };
}

const ORDER = Object.keys(ASKED).sort((a, b) => {
  const at = (id: string) => {
    const f = section(id);
    if (!f) return 0;
    return PAGES.indexOf(f.page) * 100 + f.page.sections.indexOf(f.section);
  };
  return at(a) - at(b);
});

export interface GuideMatch {
  confident: boolean;
  ids: string[];
}

/** A phrase from the ways people ask whose words are all in the question is a confident match. */
export function matchGuide(question: string, asked = false): GuideMatch | null {
  const said = new Set(words(question));
  const sure = ORDER.filter((id) =>
    (ASKED[id] ?? []).some((way) => words(way).every((w) => said.has(w))),
  ).slice(0, 3);
  if (sure.length > 0) return { confident: true, ids: sure };
  let best: { id: string; shared: number } | null = null;
  for (const id of ORDER) {
    const f = section(id);
    if (!f) continue;
    const text = new Set(words(`${f.section.heading} ${JSON.stringify(f.section.blocks)}`));
    const shared = [...said].filter((w) => w.length > 3 && text.has(w)).length;
    if (!best || shared > best.shared) best = { id, shared };
  }
  if (best && (asked || (best.shared >= 2 && HOW_TO.test(question.trim())))) {
    return { confident: asked, ids: [best.id] };
  }
  return null;
}

export function helpView(match: GuideMatch, kind: HelpKind, onlineOn: boolean): HelpView {
  const sections = match.ids.flatMap((id) => cite(id) ?? []);
  const first = match.ids[0] ? section(match.ids[0]) : null;
  return {
    kind,
    sections,
    opening: first ? first.page.opening : [],
    cited: [],
    onlineOn,
  };
}

/** What a written help answer says for a section: its first step, its first control in bold. */
export function writtenFor(id: string): string {
  const c = cite(id);
  const step = c?.steps[0] ?? [];
  let bolded = false;
  const text = step
    .map((p) => {
      if (!bolded && (p.type === "control" || p.type === "strong")) {
        bolded = true;
        return `**${p.text}**`;
      }
      return p.text;
    })
    .join("");
  // One bold phrase for each source cited, as the real answer has.
  return bolded ? text : `**${text}**`;
}
