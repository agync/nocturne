#!/usr/bin/env python3
"""Assemble the static site for Cloudflare.

    build.py theme <docs-theme> <fonts> <out>
        Lay out the docs theme for the renderer: the theme directory plus the site's fonts.

    build.py strings <site>
        Check the strings files in <site>/i18n/: every language has every id English has, no id
        English lacks, and no string translated from English that has since changed.

    build.py assemble --site-url URL --render <render-out> --lock <lemonade.lock> <site> <dist>
        Check the strings, write the landing, install, support, privacy and not-found pages once per language from
        their templates, copy the assets, fill in the placeholders, add the rendered docs under
        docs/ with the site origin filled in, and write _headers with the hashes of the docs'
        inline scripts.
"""

import argparse
import base64
import hashlib
import html
import json
import pathlib
import re
import shutil
import sys
import tomllib

PAGES = {"index.html": "/", "install.html": "/install", "support.html": "/support", "privacy.html": "/privacy", "404.html": "/"}
STATIC = ("favicon.svg", "robots.txt")
ICONS = ("favicon-32.png", "apple-touch-icon.png")
INLINE_SCRIPT = re.compile(r"<script(?![^>]*\bsrc=)([^>]*)>(.*?)</script>", re.S)

# The site's languages in the order the switch lists them: code, own name, html lang, og:locale.
# English is the source and stays at the root.
LANGUAGES = (
    ("de", "Deutsch", "de", "de_DE"),
    ("en", "English", "en-GB", "en_GB"),
    ("es", "Español", "es", "es_ES"),
    ("fr", "Français", "fr", "fr_FR"),
    ("it", "Italiano", "it", "it_IT"),
    ("nl", "Nederlands", "nl", "nl_NL"),
)
# Ids the templates do not name: the build uses them for the switch, the notes and lang.js.
BUILT_IN = (
    "lang.aria",
    "docs.inEnglish",
    "suggest.text",
    "suggest.link",
    "suggest.close",
    "note.english",
    "note.understanding",
)
# The globe on the language switch: it says "language" in any language, where a flag would name a
# country. The summary's own label carries the words.
GLOBE = (
    '<svg class="lang-globe" viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">'
    '<circle cx="8" cy="8" r="6.25" fill="none" stroke="currentColor" stroke-width="1.25"/>'
    '<path d="M1.75 8h12.5M8 1.75c-1.9 1.8-2.75 3.9-2.75 6.25S6.1 12.45 8 14.25'
    'M8 1.75c1.9 1.8 2.75 3.9 2.75 6.25S9.9 12.45 8 14.25" fill="none" stroke="currentColor" stroke-width="1.25"/>'
    "</svg>"
)
BLOCKS = "alternates|og_alternates|suggest_script|lang_switch|lang_foot|note"
LINE = re.compile(r"^([ \t]*)\{\{(" + BLOCKS + r")\}\}\n", re.M)
STRING = re.compile(r"\{\{t:([a-zA-Z0-9.]+)\}\}")
MESSAGE = re.compile(r'^\s+(\w+):\s*"([^"]*)"', re.M)


def theme(args: argparse.Namespace) -> None:
    out = pathlib.Path(args.out)
    shutil.rmtree(out, ignore_errors=True)
    shutil.copytree(args.theme, out)
    fonts = out / "assets" / "fonts"
    fonts.mkdir(parents=True, exist_ok=True)
    for font in pathlib.Path(args.fonts).iterdir():
        if font.suffix in (".woff2", ".txt"):
            shutil.copy2(font, fonts / font.name)


def script_hashes(docs: pathlib.Path) -> list[str]:
    hashes = set()
    for page in docs.rglob("*.html"):
        for attrs, body in INLINE_SCRIPT.findall(page.read_text(encoding="utf-8")):
            if "application/json" in attrs or "application/ld+json" in attrs:
                continue
            digest = hashlib.sha256(body.encode("utf-8")).digest()
            hashes.add("'sha256-" + base64.b64encode(digest).decode("ascii") + "'")
    return sorted(hashes)


def no_break(code: str, text: str) -> str:
    """Write the no-break spaces the copy tables leave as plain spaces."""
    text = re.sub(r"(\d) (?=(?:GB|Go|Bit|bits?)\b)", "\\1\u00a0", text)
    if code == "fr":
        text = re.sub(r" ([:;?!»])", "\u202f\\1", text)
        text = text.replace("« ", "«\u202f")
    return text


def load_strings(site: pathlib.Path) -> tuple[dict, dict, list[str]]:
    """English, every language's words by id, and each problem found."""
    i18n = site / "i18n"
    english = json.loads((i18n / "en.json").read_text(encoding="utf-8"))
    problems = []
    texts = {"en": english}
    for code, *_ in LANGUAGES:
        if code == "en":
            continue
        path = i18n / f"{code}.json"
        if not path.is_file():
            problems.append(f"{code}: no {path.name}")
            continue
        entries = json.loads(path.read_text(encoding="utf-8"))
        for ident in english:
            if ident not in entries:
                problems.append(f"{code}: missing {ident}")
            elif entries[ident].get("from") != english[ident]:
                problems.append(f"{code}: stale {ident}, the English changed since it was translated")
        for ident in entries:
            if ident not in english:
                problems.append(f"{code}: unknown {ident}")
        texts[code] = {k: no_break(code, v["text"]) for k, v in entries.items() if k in english}

    used = set(BUILT_IN)
    for name in PAGES:
        for ident in STRING.findall((site / name).read_text(encoding="utf-8")):
            used.add(ident)
            if ident not in english:
                problems.append(f"{name}: names {ident}, which en.json lacks")
    install = (site / "assets" / "js" / "install.js").read_text(encoding="utf-8")
    for key, text in MESSAGE.findall(install):
        used.add(f"js.{key}")
        if english.get(f"js.{key}") != text:
            problems.append(f"en: js.{key} differs from its message in install.js")
    for ident in english:
        if ident not in used:
            problems.append(f"en: {ident} is used nowhere")
    return english, texts, problems


def strings(args: argparse.Namespace) -> None:
    english, _, problems = load_strings(pathlib.Path(args.site))
    if problems:
        sys.exit("build.py: strings:\n  " + "\n  ".join(problems))
    print(f"build.py: strings: {len(english)} ids, {len(LANGUAGES) - 1} translations complete, none stale")


def prefix(code: str, path: str) -> str:
    return path if code == "en" else f"/{code}{path}"


def hreflang(code: str) -> str:
    return "en" if code == "en" else code


def render(template: str, page: str, code: str, texts: dict) -> str:
    words = texts[code]
    path = PAGES[page]
    _, own, html_lang, og_locale = next(x for x in LANGUAGES if x[0] == code)

    def links(opening: str) -> list[str]:
        rows = [opening]
        for other, name, other_lang, _ in LANGUAGES:
            current = ' aria-current="true"' if other == code else ""
            href = prefix(other, path)
            rows.append(f'  <li><a href="{href}" lang="{other_lang}" hreflang="{hreflang(other)}"{current}>{name}</a></li>')
        return rows + ["</ul>"]

    alternates = [
        f'<link rel="alternate" hreflang="{hreflang(c)}" href="{{{{SITE_URL}}}}{prefix(c, path)}">' for c, *_ in LANGUAGES
    ]
    alternates.append(f'<link rel="alternate" hreflang="x-default" href="{{{{SITE_URL}}}}{path}">')
    switch = [
        '<details class="lang">',
        "  <summary>",
        f'    {GLOBE}',
        f'    <span class="visually-hidden">{words["lang.aria"]}: </span><span class="lang-name">{own}</span>'
        f'<span class="lang-code" aria-hidden="true">{code.upper()}</span>',
        "  </summary>",
        *("  " + row for row in links("<ul>")),
        "</details>",
    ]
    note = f'<p class="lang-note">{words["note.english"]} {words["note.understanding"]}</p>'
    blocks = {
        "alternates": alternates,
        "og_alternates": [f'<meta property="og:locale:alternate" content="{loc}">' for c, _, _, loc in LANGUAGES if c != code],
        "suggest_script": ['<script src="/assets/js/lang.js" defer></script>'] if code == "en" else [],
        "lang_switch": switch,
        "lang_foot": links(f'<ul class="foot-langs" aria-label="{words["lang.aria"]}">'),
        "note": [] if code == "en" else [note],
    }
    text = LINE.sub(lambda m: "".join(m.group(1) + row + "\n" for row in blocks[m.group(2)]), template)

    def string(match: re.Match) -> str:
        ident = match.group(1)
        value = words[ident]
        if match.string[match.start() - 1] == '"' and ('"' in value or "<" in value):
            sys.exit(f"build.py: {code} {ident} goes in an attribute but has a quote or markup")
        return value

    text = STRING.sub(string, text)
    messages = mark = link = ""
    if code != "en":
        for ident, value in words.items():
            if ident.startswith("js."):
                key = re.sub(r"[A-Z]", lambda m: "-" + m.group(0).lower(), ident[3:])
                messages += f' data-msg-{key}="{html.escape(value)}"'
        mark = (
            '<span class="en-mark" aria-hidden="true">EN</span>'
            f'<span class="visually-hidden"> ({words["docs.inEnglish"]})</span>'
        )
        link = ' hreflang="en"'
    text = (
        text.replace("{{here_messages}}", messages)
        .replace("{{lang}}", html_lang)
        .replace("{{og_locale}}", og_locale)
        .replace("{{en_link}}", link)
        .replace("{{en_mark}}", mark)
    )
    return re.sub(r"\{\{p:(/[^}]*)\}\}", lambda m: prefix(code, m.group(1)), text)


def suggestions(texts: dict) -> str:
    offer = {}
    for code, _, html_lang, _ in LANGUAGES:
        if code != "en":
            words = texts[code]
            offer[code] = {
                "lang": html_lang,
                "text": words["suggest.text"],
                "link": words["suggest.link"],
                "close": words["suggest.close"],
            }
    return json.dumps(offer, ensure_ascii=False)


def assemble(args: argparse.Namespace) -> None:
    site = pathlib.Path(args.site)
    dist = pathlib.Path(args.dist)
    url = args.site_url.rstrip("/")
    if not re.fullmatch(r"https://[a-z0-9.-]+(:[0-9]+)?", url):
        sys.exit(f"build.py: SITE_URL must be an https origin with no path, got {url!r}")
    version = tomllib.loads(pathlib.Path(args.lock).read_text(encoding="utf-8"))["version"]
    _, texts, problems = load_strings(site)
    if problems:
        sys.exit("build.py: strings:\n  " + "\n  ".join(problems))

    shutil.rmtree(dist, ignore_errors=True)
    dist.mkdir(parents=True)
    shutil.copytree(site / "assets", dist / "assets")
    lang_js = dist / "assets" / "js" / "lang.js"
    script = lang_js.read_text(encoding="utf-8")
    if "/*@SUGGEST@*/ {}" not in script:
        sys.exit("build.py: lang.js lacks its /*@SUGGEST@*/ {} placeholder")
    lang_js.write_text(script.replace("/*@SUGGEST@*/ {}", suggestions(texts)), encoding="utf-8")

    pages = 0
    for name in PAGES:
        template = (site / name).read_text(encoding="utf-8")
        if INLINE_SCRIPT.search(template) or re.search(r"<style|\sstyle=", template):
            sys.exit(f"build.py: {name} has an inline script or style; the policy forbids it")
        for code, *_ in LANGUAGES:
            text = render(template, name, code, texts)
            text = text.replace("{{SITE_URL}}", url).replace("{{LEMONADE_VERSION}}", version)
            if "{{" in text:
                sys.exit(f"build.py: {code} {name} has an unfilled placeholder")
            out = dist / name if code == "en" else dist / code / name
            out.parent.mkdir(parents=True, exist_ok=True)
            out.write_text(text, encoding="utf-8")
            pages += 1
    for name in STATIC + ICONS:
        shutil.copy2(site / name, dist / name)

    rendered = pathlib.Path(args.render) / "docs"
    if not (rendered / "index.html").is_file():
        sys.exit(f"build.py: no rendered docs at {rendered}")
    shutil.copytree(rendered, dist / "docs")
    # The theme's head slot carries the Open Graph image, whose address must be absolute.
    for page in (dist / "docs").rglob("*.html"):
        text = page.read_text(encoding="utf-8")
        if "{{SITE_URL}}" in text:
            page.write_text(text.replace("{{SITE_URL}}", url), encoding="utf-8")

    hashes = script_hashes(dist / "docs")
    headers = (site / "_headers").read_text(encoding="utf-8")
    headers = headers.replace("@DOCS_SCRIPT_HASHES@", " ".join(hashes))
    (dist / "_headers").write_text(headers, encoding="utf-8")

    files = sum(1 for p in dist.rglob("*") if p.is_file())
    print(
        f"build.py: {dist} ready, {files} files, {pages} pages in {len(LANGUAGES)} languages, "
        f"{len(hashes)} docs script hashes, site {url}"
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="command", required=True)

    t = sub.add_parser("theme")
    t.add_argument("theme")
    t.add_argument("fonts")
    t.add_argument("out")
    t.set_defaults(run=theme)

    s = sub.add_parser("strings")
    s.add_argument("site")
    s.set_defaults(run=strings)

    a = sub.add_parser("assemble")
    a.add_argument("--site-url", required=True)
    a.add_argument("--render", required=True)
    a.add_argument("--lock", required=True)
    a.add_argument("site")
    a.add_argument("dist")
    a.set_defaults(run=assemble)

    args = parser.parse_args()
    args.run(args)


if __name__ == "__main__":
    main()
