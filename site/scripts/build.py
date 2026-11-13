#!/usr/bin/env python3
"""Assemble the static site for Cloudflare.

    build.py theme <docs-theme> <fonts> <out>
        Lay out the docs theme for the renderer: the theme directory plus the site's fonts.

    build.py assemble --site-url URL --render <render-out> --lock <lemonade.lock> <site> <dist>
        Copy the landing page and its assets into <dist>, fill in the placeholders, add the
        rendered docs under docs/ and write _headers with the hashes of the docs' inline scripts.
"""

import argparse
import base64
import hashlib
import pathlib
import re
import shutil
import sys
import tomllib

LANDING = ("index.html", "404.html", "favicon.svg", "robots.txt")
INLINE_SCRIPT = re.compile(r"<script(?![^>]*\bsrc=)([^>]*)>(.*?)</script>", re.S)


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


def assemble(args: argparse.Namespace) -> None:
    site = pathlib.Path(args.site)
    dist = pathlib.Path(args.dist)
    url = args.site_url.rstrip("/")
    if not re.fullmatch(r"https://[a-z0-9.-]+(:[0-9]+)?", url):
        sys.exit(f"build.py: SITE_URL must be an https origin with no path, got {url!r}")
    version = tomllib.loads(pathlib.Path(args.lock).read_text(encoding="utf-8"))["version"]

    shutil.rmtree(dist, ignore_errors=True)
    dist.mkdir(parents=True)
    shutil.copytree(site / "assets", dist / "assets")
    for name in LANDING:
        text = (site / name).read_text(encoding="utf-8")
        if name.endswith(".html"):
            if INLINE_SCRIPT.search(text) or re.search(r"<style|\sstyle=", text):
                sys.exit(f"build.py: {name} has an inline script or style; the policy forbids it")
            text = text.replace("{{SITE_URL}}", url).replace("{{LEMONADE_VERSION}}", version)
            if "{{" in text:
                sys.exit(f"build.py: {name} has an unfilled placeholder")
        (dist / name).write_text(text, encoding="utf-8")

    rendered = pathlib.Path(args.render) / "docs"
    if not (rendered / "index.html").is_file():
        sys.exit(f"build.py: no rendered docs at {rendered}")
    shutil.copytree(rendered, dist / "docs")

    hashes = script_hashes(dist / "docs")
    headers = (site / "_headers").read_text(encoding="utf-8")
    headers = headers.replace("@DOCS_SCRIPT_HASHES@", " ".join(hashes))
    (dist / "_headers").write_text(headers, encoding="utf-8")

    files = sum(1 for p in dist.rglob("*") if p.is_file())
    print(f"build.py: {dist} ready, {files} files, {len(hashes)} docs script hashes, site {url}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="command", required=True)

    t = sub.add_parser("theme")
    t.add_argument("theme")
    t.add_argument("fonts")
    t.add_argument("out")
    t.set_defaults(run=theme)

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
