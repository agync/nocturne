#!/usr/bin/env python3
"""Fail when two tracked paths differ only by case.

Linux keeps both files; Windows and macOS keep one, so a checkout there loses a file and the
build breaks. Script modules also count without their extension: `words.ts` and `Words.tsx` are
both imported as `./words` or `./Words`, which name one file on Windows and macOS. Usage: scripts/check_case_collisions.py [PATH...]  (default: git ls-files)
"""

import subprocess
import sys

MODULE_EXTENSIONS = (".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs")


def module_name(path):
    """The path a script import names: without its extension for a script module."""
    for ext in MODULE_EXTENSIONS:
        if path.endswith(ext):
            return path[: -len(ext)]
    return path


def collisions(paths):
    """Groups of paths, files or directories, that are the same ignoring case."""
    seen = {}
    for path in paths:
        parts = path.split("/")
        for end in range(1, len(parts) + 1):
            name = "/".join(parts[:end])
            seen.setdefault(name.lower(), set()).add(name)
    modules = {}
    for path in paths:
        name = module_name(path)
        if name != path:
            modules.setdefault(name.lower(), set()).add(path)
    for group in modules.values():
        if len({module_name(p) for p in group}) > 1:
            seen.setdefault("\0" + min(group).lower(), set()).update(group)
    groups = {tuple(sorted(names)) for names in seen.values() if len(names) > 1}
    return sorted(list(group) for group in groups)


def main(argv):
    paths = argv or subprocess.run(
        ["git", "ls-files", "-z"], check=True, capture_output=True, text=True
    ).stdout.split("\0")
    bad = collisions([p for p in paths if p])
    for group in bad:
        print("paths differ only by case: " + ", ".join(group), file=sys.stderr)
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
