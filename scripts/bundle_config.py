#!/usr/bin/env python3
"""Merge a bundle overlay into src-tauri/tauri.conf.json the way `tauri build --config` does.

Usage: scripts/bundle_config.py <linux|windows|macos> [--extra FILE] [--platform NAME]

Prints the merged configuration as JSON. Exits non-zero when the packaging rules are broken:
the base file carries a version that differs from the workspace version, or the resources do
not point at the verified Lemonade folder for the platform. The platform is the one the kind
builds by default; --platform names another (linux-aarch64, with --extra src-tauri/bundle/
linux-aarch64.json).
"""

import json
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PLATFORMS = {"linux": "linux-x86_64", "windows": "windows-x86_64", "macos": "macos-aarch64"}
OTHER_PLATFORMS = {"linux": ("linux-aarch64",)}


def merge(base, patch):
    """RFC 7396 merge patch: objects merge, everything else (arrays too) is replaced."""
    if not isinstance(patch, dict):
        return patch
    out = dict(base) if isinstance(base, dict) else {}
    for key, value in patch.items():
        if value is None:
            out.pop(key, None)
        else:
            out[key] = merge(out.get(key), value)
    return out


def workspace_version(root=ROOT):
    with open(os.path.join(root, "Cargo.toml"), encoding="utf-8") as handle:
        text = handle.read()
    section = re.search(r"\[workspace\.package\](.*?)(?:\n\[|\Z)", text, re.S)
    match = re.search(r'^version\s*=\s*"([^"]+)"', section.group(1), re.M) if section else None
    if not match:
        raise SystemExit("workspace version not found in Cargo.toml")
    return match.group(1)


def load(path):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def effective(kind, extra=None, root=ROOT):
    base = load(os.path.join(root, "src-tauri", "tauri.conf.json"))
    merged = merge(base, load(os.path.join(root, "src-tauri", "bundle", f"{kind}.json")))
    if extra:
        merged = merge(merged, load(extra))
    return merged


def problems(kind, merged, root=ROOT, platform=None):
    found = []
    version = workspace_version(root)
    declared = merged.get("version")
    if declared is not None and declared != version:
        found.append(f"tauri.conf.json version {declared} differs from workspace version {version}")
    want = f"../target/package/lemonade/{platform or PLATFORMS[kind]}/"
    resources = merged.get("bundle", {}).get("resources", {})
    if resources.get(want) != "lemonade/" or list(resources.values()).count("lemonade/") != 1:
        found.append(f"resources must map {want}, and only it, to lemonade/, got {resources}")
    if merged.get("bundle", {}).get("active") is not True:
        found.append("bundle.active is not true in the merged configuration")
    return found


def main(argv):
    if not argv or argv[0] not in PLATFORMS:
        print(__doc__, file=sys.stderr)
        return 2
    extra = argv[argv.index("--extra") + 1] if "--extra" in argv else None
    platform = argv[argv.index("--platform") + 1] if "--platform" in argv else None
    if platform not in (None, PLATFORMS[argv[0]], *OTHER_PLATFORMS.get(argv[0], ())):
        print(f"bundle config: no {platform} build for {argv[0]}", file=sys.stderr)
        return 2
    merged = effective(argv[0], extra)
    found = problems(argv[0], merged, platform=platform)
    print(json.dumps(merged, indent=2))
    print(f"version (workspace): {workspace_version()}", file=sys.stderr)
    for line in found:
        print(f"bundle config: {line}", file=sys.stderr)
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
