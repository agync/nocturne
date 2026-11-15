#!/usr/bin/env python3
"""Fail when the built interface holds the development-only preview or the server.

Usage: scripts/check_ui_dist.py DIR

The release binary embeds the interface compressed, so the package check cannot read its text.
This runs on the plain files in `ui/dist` before the app is built.
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from check_tree import PREVIEW_MARKERS, walk  # noqa: E402


def main(argv=None):
    args = sys.argv[1:] if argv is None else argv
    if len(args) != 1 or not os.path.isdir(args[0]):
        print("usage: check_ui_dist.py DIR", file=sys.stderr)
        return 2
    found = []
    for path in walk(args[0]):
        if os.path.islink(path) or not os.path.isfile(path):
            continue
        with open(path, "rb") as handle:
            data = handle.read()
        if any(marker in data for marker in PREVIEW_MARKERS):
            found.append(os.path.relpath(path, args[0]))
    if found:
        print(f"the development-only real preview or the server is in the built interface: {found[:5]}")
        return 1
    print("no development-only real preview or server in the built interface")
    return 0


if __name__ == "__main__":
    sys.exit(main())
