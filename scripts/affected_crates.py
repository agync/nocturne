#!/usr/bin/env python3
"""Which workspace packages a change needs tested.

Usage: scripts/affected_crates.py [--metadata FILE] < changed-paths
       scripts/affected_crates.py --all

Reads the changed paths, one per line, and prints three `key=value` lines for $GITHUB_OUTPUT:

  scope=all|some|none
  packages=--workspace | -p a -p b | (empty)
  app=true|false      whether nocturne-app is among them (rust/test runs it a second time)

A changed file selects the package it belongs to and every package that depends on it, through
normal, dev and build dependencies. A file another package reads (an include_str!, a #[path] or
a test that opens it) selects that package too, from the tables below. A change to what every
build reads (the lock file, the workspace manifest, the toolchain, cargo's and nextest's
configuration, the workflows, the Makefile or this script) selects the whole workspace, and so
does a file under a Rust tree that no package owns. Any other path selects nothing.

The packages and their dependencies come from `cargo metadata --format-version 1 --no-deps`, or
from FILE. The reasons go to standard error.
"""

import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# A change here runs the whole workspace.
EVERYTHING = re.compile(
    r"^(Cargo\.(toml|lock)$|rust-toolchain(\.toml)?$|\.cargo/|\.config/nextest\.toml$"
    r"|clippy\.toml$|Makefile$|\.github/(workflows|actions)/"
    r"|scripts/(affected_crates\.py|ci-areas\.sh)$)"
)

# A file outside every package under one of these trees is a Rust file nobody claimed.
RUST_TREES = re.compile(r"^(crates|src-tauri|phone)/")

# Files a package compiles in from outside its own folder: a change selects the package and its
# dependents, as a change to its own source does.
BUILT_FROM = (
    # The guide and the interface's copy, embedded in the help and the menus.
    (r"^docs/guide/", "nocturne-app"),
    (r"^ui/src/(copy|components/ControlChip|engine/mock/seed)/", "nocturne-app"),
    (r"^ui/src/copy/en-GB/phone-health\.json$", "tauri-plugin-nocturne-calendar"),
    (r"^helpers\.lock$", "nocturne-helpers"),
)

# Files only a package's tests read: a change selects the package alone.
TESTS_READ = (
    # The drift, IPC and capability tests compare the app with the interface's sources.
    (r"^ui/", "nocturne-app"),
    (r"^phone/src-tauri/src/sync/", "nocturne-app"),
    (r"^ui/src/engine/bindings/", "nocturne-contract"),
    (r"^CHANGELOG\.md$", "nocturne-channel"),
    (r"^src-tauri/tests/fixtures/remote-vectors\.json$", "nocturne-channel"),
    (r"^crates/contract/tests/fixtures/", "nocturne-channel"),
    (r"^crates/contract/tests/fixtures/", "nocturne-app"),
    # A test that every program is started without a window reads every crate's source.
    (r"^(crates/[^/]+/src|src-tauri/src)/", "nocturne-lemonade"),
)


def load_metadata(path=None):
    if path:
        with open(path, encoding="utf-8") as f:
            return json.load(f)
    out = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps", "--offline"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    return json.loads(out)


def workspace(metadata):
    """Each package's folder relative to the workspace root, and the packages that depend on it."""
    root = metadata["workspace_root"].replace("\\", "/").rstrip("/")
    dirs, users = {}, {}
    names = {p["name"] for p in metadata["packages"]}
    for p in metadata["packages"]:
        manifest = p["manifest_path"].replace("\\", "/")
        rel = os.path.dirname(manifest)[len(root) + 1 :]
        dirs[p["name"]] = rel + "/" if rel else ""
        users.setdefault(p["name"], set())
    for p in metadata["packages"]:
        for dep in p["dependencies"]:
            if dep["name"] in names and dep["name"] != p["name"]:
                users[dep["name"]].add(p["name"])
    return dirs, users


def dependents(names, users):
    found, todo = set(names), list(names)
    while todo:
        for user in users[todo.pop()]:
            if user not in found:
                found.add(user)
                todo.append(user)
    return found


def owner(path, dirs):
    best = None
    for name, d in dirs.items():
        if d and path.startswith(d) and (best is None or len(d) > len(dirs[best])):
            best = name
    return best


def affected(paths, metadata):
    """The packages to test, or None for the whole workspace, and the reason for each."""
    dirs, users = workspace(metadata)
    reasons = {}

    def add(names, why):
        for name in sorted(names):
            reasons.setdefault(name, why)

    for path in paths:
        if EVERYTHING.match(path):
            return None, {"*": f"{path} is read by every build"}
        built = {name for pattern, name in BUILT_FROM if re.match(pattern, path)}
        tests = {name for pattern, name in TESTS_READ if re.match(pattern, path)}
        own = owner(path, dirs)
        if own:
            built.add(own)
        for name in built | tests:
            if name not in dirs:
                raise SystemExit(f"affected_crates.py names {name}, which is not in the workspace")
        if not built and not tests:
            if RUST_TREES.match(path):
                return None, {"*": f"{path} is under a Rust tree but in no package"}
            continue
        add(built, f"{path} is in its source")
        add(dependents(built, users) - built, f"it depends on a package {path} is in")
        add(tests, f"its tests read {path}")
    return set(reasons), reasons


def render(selected):
    if selected is None:
        return "scope=all\npackages=--workspace\napp=true\n"
    if not selected:
        return "scope=none\npackages=\napp=false\n"
    flags = " ".join(f"-p {name}" for name in sorted(selected))
    app = "true" if "nocturne-app" in selected else "false"
    return f"scope=some\npackages={flags}\napp={app}\n"


def main(argv=None):
    args = sys.argv[1:] if argv is None else argv
    if args == ["--all"]:
        sys.stdout.write(render(None))
        return 0
    metadata_file = None
    if len(args) == 2 and args[0] == "--metadata":
        metadata_file = args[1]
    elif args:
        print("usage: affected_crates.py [--all | --metadata FILE] < changed-paths", file=sys.stderr)
        return 2
    paths = [line.strip() for line in sys.stdin.read().splitlines() if line.strip()]
    selected, reasons = affected(paths, load_metadata(metadata_file))
    for name, why in sorted(reasons.items()):
        print(f"{name}: {why}", file=sys.stderr)
    sys.stdout.write(render(selected))
    return 0


if __name__ == "__main__":
    sys.exit(main())
