#!/usr/bin/env python3
"""Build THIRD-PARTY-NOTICES.txt for a Nocturne package, offline.

Sources: the Rust crates the app links (cargo metadata, normal dependencies only, for the
packaged platform), the production npm packages of the interface (which include the bundled
fonts), the private Lemonade bundle, and the two connection helpers Away from home downloads on
consent (cloudflared and the tailnet helper, with the go-licenses report of the Go modules it is
built from). Every licence must be on the allow list; anything else
or unknown fails, naming the package.

Usage:
  scripts/notices.py [--check] [--out DIR] [--lemonade DIR] [--target TRIPLE]
                     [--tailnet-licenses FILE]
  scripts/notices.py --prune-libs APPDIR_LIB_DIR
  scripts/notices.py --bundled-libs APPDIR_LIB_DIR [--out DIR] [--sources FILE]

The second form lists the shared libraries an AppImage carries and takes their licences from
the system package database, writing BUNDLED-LIBRARIES.txt (see scripts/bundled_libs.py).
"""

import argparse
import glob
import hashlib
import json
import os
import re
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DEFAULT_OUT = os.path.join(ROOT, "target", "package", "notices")
DEFAULT_TARGET = "x86_64-unknown-linux-gnu"
APP_CRATE = "nocturne-app"

ALLOWED = {
    "MIT",
    "Apache-2.0",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "Zlib",
    "OFL-1.1",
    "Unicode-3.0",
    "MPL-2.0",
    "CC0-1.0",
    "BSL-1.0",
    "0BSD",
    "Apache-2.0 WITH LLVM-exception",
    "CDLA-Permissive-2.0",
    "Unlicense",
}

LICENCE_FILE_GLOBS = ("LICENSE*", "LICENCE*", "COPYING*", "NOTICE*", "UNLICENSE*")

FONT_NAMES = {
    "@fontsource-variable/newsreader": "Newsreader",
    "@fontsource/ibm-plex-sans": "IBM Plex Sans",
    "@fontsource/ibm-plex-mono": "IBM Plex Mono",
}

# Works carried in the app that are not packages: each is listed with its own licence.
WORKS = [
    {
        "name": "BIP-39 English word list",
        "license": "MIT",
        "holder": "Marek Palatinus, Pavol Rusnak, Aaron Voisine, Sean Bowe",
        "source": "https://github.com/bitcoin/bips/blob/"
        "b34d9f751791d4feb22feff4f2ed00bcac83961b/bip-0039/english.txt",
        "use": "the check words shown when a phone is added (Away from home)",
    },
]

# The connection helpers are downloaded on consent, never packaged; their notices still ship.
HELPERS = [
    {
        "name": "cloudflared",
        "license": "Apache-2.0",
        "holder": "Cloudflare, Inc.",
        "source": "https://github.com/cloudflare/cloudflared",
        "use": "the Simple connection for Away from home",
    },
    {
        "name": "tailscale.com (the Go module built into nocturne-tailnet)",
        "license": "BSD-3-Clause",
        "holder": "Tailscale Inc. and AUTHORS",
        "source": "https://github.com/tailscale/tailscale",
        "use": "the Most private connection for Away from home",
    },
]

# Standard texts, used only for a package that ships no licence file of its own. The holder
# line comes from the package's own metadata.
TEMPLATES = {
    "MIT": """Permission is hereby granted, free of charge, to any person obtaining a copy of this
software and associated documentation files (the "Software"), to deal in the Software without
restriction, including without limitation the rights to use, copy, modify, merge, publish,
distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the
Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or
substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT
NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.""",
    "Apache-2.0": """Licensed under the Apache License, Version 2.0 (the "License"); you may not use this
software except in compliance with the License. You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software distributed under the
License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
either express or implied. See the License for the specific language governing permissions
and limitations under the License.""",
    "BSD-3-Clause": """Redistribution and use in source and binary forms, with or without modification, are
permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this list of
   conditions and the following disclaimer.
2. Redistributions in binary form must reproduce the above copyright notice, this list of
   conditions and the following disclaimer in the documentation and/or other materials
   provided with the distribution.
3. Neither the name of the copyright holder nor the names of its contributors may be used to
   endorse or promote products derived from this software without specific prior written
   permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR
IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND
FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR
CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR
OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
POSSIBILITY OF SUCH DAMAGE.""",
    "Unlicense": """This is free and unencumbered software released into the public domain. Anyone is free
to copy, modify, publish, use, compile, sell, or distribute this software, either in source
code form or as a compiled binary, for any purpose, commercial or non-commercial, and by any
means. See https://unlicense.org for the full text.""",
    "MPL-2.0": """This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
The full text is at https://www.mozilla.org/MPL/2.0/ and the source code is available from
the repository named above.""",
}


class LicenceError(Exception):
    pass


def tokenize(expr):
    expr = expr.replace("/", " OR ")
    return re.findall(r"\(|\)|[^\s()]+", expr)


def evaluate(expr, allowed=ALLOWED, atom_ok=None):
    """True when the SPDX expression is satisfied.

    OR needs any side allowed, AND needs both, `X WITH Y` is judged as the whole phrase. An
    atom is judged by `atom_ok(name)` when given, else by membership in `allowed`.
    """
    tokens = tokenize(expr)
    pos = 0

    def peek():
        return tokens[pos] if pos < len(tokens) else None

    def take():
        nonlocal pos
        tok = tokens[pos]
        pos += 1
        return tok

    def atom():
        tok = peek()
        if tok is None:
            raise LicenceError(f"incomplete licence expression: {expr!r}")
        if tok == "(":
            take()
            value = or_expr()
            if peek() != ")":
                raise LicenceError(f"unbalanced licence expression: {expr!r}")
            take()
            return value
        take()
        name = tok.rstrip("+") if tok.rstrip("+") in allowed else tok
        if peek() is not None and peek().upper() == "WITH":
            take()
            exc = take()
            name = f"{name} WITH {exc}"
        return atom_ok(name) if atom_ok else name in allowed

    def and_expr():
        value = atom()
        while peek() is not None and peek().upper() == "AND":
            take()
            right = atom()
            value = value and right
        return value

    def or_expr():
        value = and_expr()
        while peek() is not None and peek().upper() == "OR":
            take()
            right = and_expr()
            value = value or right
        return value

    result = or_expr()
    if pos != len(tokens):
        raise LicenceError(f"unparsed licence expression: {expr!r}")
    return result


def licence_files(directory):
    found = []
    for pattern in LICENCE_FILE_GLOBS:
        found.extend(glob.glob(os.path.join(directory, pattern)))
    return sorted(p for p in set(found) if os.path.isfile(p))


def read_text(path):
    with open(path, encoding="utf-8", errors="replace") as handle:
        return handle.read().strip("\n")


def rust_packages(metadata):
    """The third-party crates reachable from the app through normal dependencies."""
    by_id = {p["id"]: p for p in metadata["packages"]}
    members = set(metadata["workspace_members"])
    root = next(
        (i for i in members if by_id[i]["name"] == APP_CRATE),
        None,
    )
    if root is None:
        raise LicenceError(f"workspace member {APP_CRATE} not found")
    nodes = {n["id"]: n for n in metadata["resolve"]["nodes"]}
    seen, stack = set(), [root]
    while stack:
        current = stack.pop()
        if current in seen:
            continue
        seen.add(current)
        for dep in nodes[current]["deps"]:
            kinds = dep.get("dep_kinds") or [{"kind": None}]
            if any(k.get("kind") is None for k in kinds):
                stack.append(dep["pkg"])
    out = []
    for pid in sorted(seen):
        pkg = by_id[pid]
        if pid in members or pkg.get("source") is None:
            continue
        out.append(
            {
                "kind": "crate",
                "name": pkg["name"],
                "version": pkg["version"],
                "license": pkg.get("license") or "",
                "dir": os.path.dirname(pkg["manifest_path"]),
                "license_file": pkg.get("license_file"),
                "holder": ", ".join(pkg.get("authors") or []),
                "source": pkg.get("repository") or pkg.get("homepage") or "",
            }
        )
    return sorted(out, key=lambda p: (p["name"], p["version"]))


def npm_packages(tree):
    """Production npm packages from `npm ls --omit=dev --all --json`."""
    found = {}

    def walk(deps):
        for name, info in (deps or {}).items():
            path = info.get("path")
            if path and path not in found:
                found[path] = name
            walk(info.get("dependencies"))

    walk(tree.get("dependencies"))
    out = []
    for path, name in sorted(found.items(), key=lambda kv: kv[1]):
        manifest = os.path.join(path, "package.json")
        try:
            with open(manifest, encoding="utf-8") as handle:
                meta = json.load(handle)
        except OSError:
            raise LicenceError(f"npm package {name} is not installed at {path}")
        lic = meta.get("license")
        if isinstance(lic, dict):
            lic = lic.get("type")
        out.append(
            {
                "kind": "npm",
                "name": meta.get("name", name),
                "version": meta.get("version", ""),
                "license": lic or "",
                "dir": path,
                "license_file": None,
                "holder": npm_holder(meta.get("author")),
                "source": npm_source(meta.get("repository")),
            }
        )
    return sorted(out, key=lambda p: (p["name"], p["version"]))


def npm_holder(author):
    if isinstance(author, dict):
        return author.get("name", "")
    return re.sub(r"\s*[<(].*$", "", author or "")


def npm_source(repository):
    if isinstance(repository, dict):
        repository = repository.get("url", "")
    return (repository or "").removeprefix("git+")


def check_policy(packages):
    """Return a list of human-readable failures; empty when every licence is allowed."""
    failures = []
    for pkg in packages:
        label = f"{pkg['name']} {pkg['version']}"
        if not pkg["license"]:
            failures.append(f"{label}: no licence declared")
            continue
        try:
            ok = evaluate(pkg["license"])
        except LicenceError as err:
            failures.append(f"{label}: {err}")
            continue
        if not ok:
            failures.append(f"{label}: licence {pkg['license']!r} is not on the allow list")
    return failures


def package_texts(pkg):
    files = licence_files(pkg["dir"])
    if pkg.get("license_file"):
        extra = os.path.join(pkg["dir"], pkg["license_file"])
        if os.path.isfile(extra) and extra not in files:
            files.append(extra)
    return [(os.path.basename(f), read_text(f)) for f in files]


def fallback_text(pkg):
    """A standard text for a package with no licence file, naming its holder and source."""
    names = [t for t in tokenize(pkg["license"]) if t in TEMPLATES]
    head = []
    if pkg.get("holder"):
        head.append(f"Copyright held by: {pkg['holder']}")
    if pkg.get("source"):
        head.append(f"Source: {pkg['source']}")
    body = TEMPLATES[names[0]] if names else f"Licence: {pkg['license']}."
    return "\n".join(head + ["", body]).strip("\n")


class TextPool:
    """Licence texts printed once when several packages ship them byte for byte."""

    def __init__(self, groups):
        counts = {}
        for entries in groups:
            for _, body in entries:
                counts[digest_of(body)] = counts.get(digest_of(body), 0) + 1
        self.numbers = {}
        for digest, n in counts.items():
            if n > 1:
                self.numbers[digest] = len(self.numbers) + 1
        self.bodies = {}

    def render(self, entries):
        lines = []
        for name, body in entries:
            digest = digest_of(body)
            if digest in self.numbers:
                self.bodies[digest] = body
                lines += ["", f"--- {name}: shared text {self.numbers[digest]} below"]
            else:
                lines += ["", f"--- {name}", body]
        return lines

    def appendix(self, rule, title):
        lines = [rule, title, rule, ""]
        for digest, number in sorted(self.numbers.items(), key=lambda kv: kv[1]):
            lines += [f"[{number}]", self.bodies.get(digest, ""), ""]
        return lines


TAILNET_REPORT_MISSING = (
    "The Go modules nocturne-tailnet is built from, with their licences, are listed in the go-licenses\n"
    "report published with the release (tailnet-licenses.txt)."
)


def build_notices(packages, lemonade_text, lemonade_version="", tailnet_report=""):
    """The notices document. Fonts and Lemonade get their own named sections.

    A licence text that several packages ship byte for byte (Apache-2.0 above all) is printed
    once in a shared section and referred to by number; texts with their own copyright lines
    are printed with their package.
    """
    rule = "=" * 72
    texts = {pkg_key(p): package_texts(p) or [("standard text", fallback_text(p))] for p in packages}
    pool = TextPool(texts.values())

    def render(pkg):
        return pool.render(texts[pkg_key(pkg)])

    lines = [
        "Nocturne third-party notices",
        "",
        "Nocturne itself is licensed under the Apache License 2.0 (see LICENSE).",
        "It includes the software below. Each entry gives the licence the package",
        "declares, then the licence texts the package ships. Texts shipped identically",
        "by several packages are printed once, in the last section.",
        "",
    ]
    fonts = [p for p in packages if p["name"] in FONT_NAMES]
    others = [p for p in packages if p["name"] not in FONT_NAMES]

    lines += [rule, "Fonts (SIL Open Font License 1.1)", rule, ""]
    for pkg in fonts:
        lines += [f"{FONT_NAMES[pkg['name']]}  ({pkg['name']} {pkg['version']}, {pkg['license']})"]
        lines += render(pkg) + [""]

    lines += [rule, "Lemonade (private local engine)", rule, ""]
    lines += [f"Lemonade {lemonade_version}".strip(), "", lemonade_text, ""]

    lines += [rule, "Word lists", rule, ""]
    for work in WORKS:
        lines += [f"{work['name']}  ({work['license']}), for {work['use']}", fallback_text(work), ""]

    lines += [rule, "Connection helpers (downloaded only if you turn on Away from home)", rule, ""]
    for work in HELPERS:
        lines += [f"{work['name']}  ({work['license']}), for {work['use']}", fallback_text(work), ""]
    lines += [
        "Go modules built into nocturne-tailnet (go-licenses report: module, licence URL, licence):",
        tailnet_report.strip() or TAILNET_REPORT_MISSING,
        "",
    ]

    lines += [rule, "Libraries", rule, ""]
    for pkg in others:
        lines += [f"{pkg['name']} {pkg['version']}  ({pkg['kind']}, {pkg['license']})"]
        lines += render(pkg) + [""]

    lines += pool.appendix(rule, "Shared licence texts")
    return "\n".join(lines).rstrip("\n") + "\n"


def pkg_key(pkg):
    return (pkg["kind"], pkg["name"], pkg["version"])


def digest_of(text):
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def run(cmd, cwd=None):
    # Cargo and npm write UTF-8; Windows would otherwise decode with its local code page.
    return subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, encoding="utf-8")


def load_rust(target):
    proc = run(
        [
            "cargo",
            "metadata",
            "--locked",
            "--offline",
            "--format-version",
            "1",
            "--filter-platform",
            target,
        ],
        cwd=ROOT,
    )
    if proc.returncode != 0:
        raise LicenceError(f"cargo metadata failed: {proc.stderr.strip()}")
    return json.loads(proc.stdout)


def load_npm():
    ui = os.path.join(ROOT, "ui")
    if not os.path.isdir(os.path.join(ui, "node_modules")):
        raise LicenceError("ui/node_modules is missing: run npm --prefix ui ci")
    proc = run(["npm", "ls", "--omit=dev", "--all", "--json", "--long", "--offline"], cwd=ui)
    if not proc.stdout.strip():
        raise LicenceError(f"npm ls failed: {proc.stderr.strip()}")
    return json.loads(proc.stdout)


def lemonade_dir(arg):
    if arg:
        return arg
    return os.path.join(ROOT, "target", "package", "lemonade", "linux-x86_64")


def lemonade_version():
    path = os.path.join(ROOT, "lemonade.lock")
    try:
        with open(path, encoding="utf-8") as handle:
            m = re.search(r'^version = "([^"]+)"', handle.read(), re.M)
        return m.group(1) if m else ""
    except OSError:
        return ""


def sha256(path):
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--check", action="store_true", help="check licences, write nothing")
    parser.add_argument("--out", default=DEFAULT_OUT)
    parser.add_argument("--lemonade", default=None)
    parser.add_argument("--target", default=DEFAULT_TARGET)
    parser.add_argument("--tailnet-licenses", default=None, help="the go-licenses report of the Go modules built into nocturne-tailnet")
    parser.add_argument("--bundled-libs", default=None)
    parser.add_argument("--prune-libs", default=None, help="remove the unused modules from an AppDir lib folder")
    parser.add_argument("--sources", default=None, help="write the source packages of bundled libraries here")
    args = parser.parse_args(argv)

    try:
        if args.prune_libs:
            import bundled_libs

            removed = bundled_libs.prune(args.prune_libs)
            print(f"removed {len(removed)} unused library files: {', '.join(removed)}")
            return 0
        if args.bundled_libs:
            import bundled_libs

            target, packages = bundled_libs.write(args.bundled_libs, args.out, args.sources)
            print(f"{target}: {len(packages)} system packages")
            return 0

        packages = rust_packages(load_rust(args.target)) + npm_packages(load_npm())
        failures = check_policy(packages) + [
            f"{w['name']}: licence {w['license']} is not allowed" for w in WORKS + HELPERS if not evaluate(w["license"])
        ]
        if failures:
            print("licence check failed:", file=sys.stderr)
            for failure in failures:
                print(f"  {failure}", file=sys.stderr)
            return 1
        lemon = lemonade_dir(args.lemonade)
        lemon_license = os.path.join(lemon, "LICENSE")
        if not os.path.isfile(lemon_license):
            raise LicenceError(f"Lemonade licence not found at {lemon_license}")
        if args.check:
            print(f"notices check ok: {len(packages)} packages, all licences allowed")
            return 0
        report = args.tailnet_licenses or os.path.join(args.out, "tailnet-licenses.txt")
        report_text = read_text(report) if os.path.isfile(report) else ""
        text = build_notices(packages, read_text(lemon_license), lemonade_version(), report_text)
        os.makedirs(args.out, exist_ok=True)
        dest = os.path.join(args.out, "THIRD-PARTY-NOTICES.txt")
        with open(dest, "w", encoding="utf-8") as handle:
            handle.write(text)
        print(f"{dest}: {len(packages)} packages, {len(text)} bytes")
        return 0
    except LicenceError as err:
        print(f"notices: {err}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
