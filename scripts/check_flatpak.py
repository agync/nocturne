#!/usr/bin/env python3
"""Check that the Flatpak files agree with the rest of the repository.

Usage: scripts/check_flatpak.py [ROOT]

- The engine archives in the manifest are the ones lemonade.lock pins, for each architecture.
- The newest release in the metainfo is the workspace version.
- The app id is the same in the manifest, the AetherPak config, the metainfo and the desktop entry.
- The finish-args in the manifest are exactly the ones the contribution page explains.

Prints each problem and exits non-zero when there is one. Run by make check through its tests.
"""

import os
import re
import sys
import tomllib
import xml.etree.ElementTree as ET

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import bundle_config  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
APP_ID = "is.nocturne.Nocturne"
MANIFEST = f"flatpak/{APP_ID}.yml"
METAINFO = f"flatpak/{APP_ID}.metainfo.xml"
DESKTOP = f"flatpak/{APP_ID}.desktop"
AETHERPAK = "flatpak/aetherpak.yaml"
DOC = "docs/contribution/flatpak.md"
ARCHES = {"x86_64": "linux-x86_64", "aarch64": "linux-aarch64"}


def read(root, path):
    with open(os.path.join(root, path), encoding="utf-8") as handle:
        return handle.read()


def scalar(text, key):
    match = re.search(rf"^{re.escape(key)}:\s*(\S+)\s*$", text, re.M)
    return match.group(1).strip("'\"") if match else None


def finish_args(manifest):
    block = re.search(r"^finish-args:\n((?:[ \t]+-.*\n|[ \t]*#.*\n)+)", manifest, re.M)
    if not block:
        return []
    return re.findall(r"^[ \t]+-[ \t]+(\S+)", block.group(1), re.M)


def archives(manifest):
    """The archive sources of the manifest as (arches, url, sha256)."""
    found = []
    for chunk in re.split(r"^[ \t]+- type: ", manifest, flags=re.M)[1:]:
        if not chunk.startswith("archive"):
            continue
        arches = re.findall(r"^[ \t]+- (x86_64|aarch64)\s*$", chunk, re.M)
        url = re.search(r"^[ \t]+url: (\S+)", chunk, re.M)
        sha = re.search(r"^[ \t]+sha256: (\S+)", chunk, re.M)
        found.append((tuple(arches), url and url.group(1), sha and sha.group(1)))
    return found


def documented_args(doc):
    return re.findall(r"^\| `(--[^`]+)` \|", doc, re.M)


def problems(root=ROOT):
    out = []
    manifest = read(root, MANIFEST)

    with open(os.path.join(root, "lemonade.lock"), "rb") as handle:
        lock = tomllib.load(handle)
    base = lock["base_url"]
    engine = [a for a in archives(manifest) if a[1] and "lemonade-embeddable" in a[1]]
    for arch, platform in ARCHES.items():
        asset = lock["assets"].get(platform)
        want = (f"{base}/{asset['file']}", asset["sha256"]) if asset else None
        have = [(url, sha) for arches, url, sha in engine if arch in arches]
        if want is None:
            if have:
                out.append(f"{MANIFEST}: an engine archive for {arch}, which lemonade.lock does not pin")
        elif have != [want]:
            out.append(f"{MANIFEST}: the {arch} engine archive is not {want[0]} with sha256 {want[1]}")

    version = bundle_config.workspace_version(root)
    meta = ET.fromstring(read(root, METAINFO))
    release = meta.find("releases/release")
    if release is None or release.get("version") != version:
        out.append(f"{METAINFO}: the newest release is not the workspace version {version}")

    ids = {
        MANIFEST: scalar(manifest, "app-id"),
        AETHERPAK: scalar(read(root, AETHERPAK), "app_id"),
        METAINFO: meta.findtext("id"),
    }
    launchable = meta.findtext("launchable")
    if launchable != f"{APP_ID}.desktop":
        out.append(f"{METAINFO}: the launchable is not {APP_ID}.desktop")
    if f"Icon={APP_ID}\n" not in read(root, DESKTOP):
        out.append(f"{DESKTOP}: the icon is not {APP_ID}")
    for path, value in ids.items():
        if value != APP_ID:
            out.append(f"{path}: the app id is {value}, not {APP_ID}")

    args = finish_args(manifest)
    documented = documented_args(read(root, DOC))
    if not args:
        out.append(f"{MANIFEST}: no finish-args")
    for arg in sorted(set(args) - set(documented)):
        out.append(f"{DOC}: {arg} is in the manifest but not explained")
    for arg in sorted(set(documented) - set(args)):
        out.append(f"{DOC}: {arg} is explained but not in the manifest")
    return out


def main():
    root = sys.argv[1] if len(sys.argv) > 1 else ROOT
    found = problems(root)
    for line in found:
        print(line)
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())
