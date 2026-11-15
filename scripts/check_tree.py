#!/usr/bin/env python3
"""Inspect an unpacked Nocturne package tree. Used by scripts/check-package.sh.

Usage: scripts/check_tree.py ROOT --version V [--kind appimage|deb|rpm|app|tree] [--manifest FILE]
                             [--requires FILE] [--builder-home DIR] [--sources FILE]

Prints one line per check and exits non-zero when any check failed.
"""

import argparse
import fnmatch
import hashlib
import os
import plistlib
import re
import struct
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import bundled_libs  # noqa: E402

# Libraries a package must declare: the tray library and what the private engine links.
REQUIRED_DEPENDS = {
    "deb": ("libayatana-appindicator3-1", "libssl3", "libzstd1", "libdrm-amdgpu1"),
    "rpm": ("libayatana-appindicator-gtk3", "openssl-libs", "libzstd", "libdrm"),
}
ICON_SIZES = (32, 128, 256)
FORBIDDEN_PARTS = (".agents", "vendor/", "/home/")
HOME_PATTERN = re.compile(rb"/home/")
MAC_HOME_PATTERN = re.compile(rb"/Users/[^/\s]")
APP_IDENTIFIER = "app.nocturne.desktop"
# The development-only real preview (the `preview` feature) and the experimental server (the
# `server` feature, shipped only as a container image) are never in a desktop package: their
# binaries have these names, the preview prints the first marker, and their interfaces keep the
# token under the second and third.
PREVIEW_BINARY = ("nocturne-preview", "nocturne-server")
PREVIEW_MARKERS = (b"nocturne-preview-dev-only", b"nocturne.preview.token", b"nocturne.server.token")


def walk(root):
    for base, dirs, files in os.walk(root):
        dirs.sort()
        for name in sorted(files + [d for d in dirs if os.path.islink(os.path.join(base, d))]):
            yield os.path.join(base, name)


def read_text(path):
    with open(path, encoding="utf-8", errors="replace") as handle:
        return handle.read()


def png_size(path):
    with open(path, "rb") as handle:
        head = handle.read(24)
    if head[:8] != b"\x89PNG\r\n\x1a\n":
        return None
    return struct.unpack(">II", head[16:24])


def is_text(data):
    return b"\0" not in data[:4096]


def sha256(path):
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def read_manifest(path):
    entries = {}
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            line = line.rstrip("\n")
            if not line:
                continue
            digest, _, name = line.partition("  ")
            entries[os.path.normpath(name)] = digest
    return entries


class Report:
    def __init__(self):
        self.failures = []

    def ok(self, message):
        print(f"ok    {message}")

    def fail(self, message):
        self.failures.append(message)
        print(f"FAIL  {message}")


def check(root, kind, manifest, requires, report, builder_home=None, sources=None, version=None):
    files = [p for p in walk(root)]
    rel = {os.path.relpath(p, root): p for p in files}

    if kind == "app":
        binaries = [r for r in rel if r == os.path.join("Contents", "MacOS", "Nocturne")]
        if binaries and os.stat(rel[binaries[0]]).st_mode & 0o111 == 0o111:
            report.ok(f"binary present and executable: {binaries[0]}")
        else:
            report.fail("no executable Contents/MacOS/Nocturne")
    else:
        binaries = [r for r in rel if r.endswith("usr/bin/nocturne")]
        if binaries:
            report.ok(f"binary present: {binaries[0]}")
        else:
            report.fail("no usr/bin/nocturne binary")

    lemond = [r for r in rel if r.endswith("lemonade/lemond")]
    if not lemond:
        report.fail("no lemonade/lemond in the package")
        lemon_dir = None
    else:
        lemon_dir = os.path.dirname(lemond[0])
        mode = os.stat(rel[lemond[0]]).st_mode
        if mode & 0o111 == 0o111:
            report.ok(f"lemond is executable: {lemond[0]}")
        else:
            report.fail(f"lemond is not executable (mode {mode & 0o777:04o}): {lemond[0]}")
        if os.path.join(os.path.dirname(lemon_dir), "LICENSE") in rel:
            report.ok("Nocturne LICENSE present")
        else:
            report.fail("Nocturne's own LICENSE is missing")
        if os.path.join(lemon_dir, "LICENSE") in rel:
            report.ok("lemonade/LICENSE present")
        else:
            report.fail("lemonade/LICENSE missing")
        notices = os.path.join(os.path.dirname(lemon_dir), "notices", "THIRD-PARTY-NOTICES.txt")
        if notices in rel and os.path.getsize(rel[notices]) > 0:
            report.ok("notices/THIRD-PARTY-NOTICES.txt present")
        else:
            report.fail("notices/THIRD-PARTY-NOTICES.txt missing or empty")
        if kind == "appimage":
            extra = os.path.join(os.path.dirname(lemon_dir), "notices", "BUNDLED-LIBRARIES.txt")
            if extra in rel and os.path.getsize(rel[extra]) > 0:
                report.ok("notices/BUNDLED-LIBRARIES.txt present")
            else:
                report.fail("notices/BUNDLED-LIBRARIES.txt missing (the AppImage carries system libraries)")

    desktops = [r for r in rel if r.endswith(".desktop")]
    if kind == "app":
        desktops = []
        check_app_bundle(root, rel, version, report)
    elif not desktops:
        report.fail("no desktop entry")
    for d in desktops:
        text = read_text(rel[d])
        match = re.search(r"^Exec=(.*)$", text, re.M)
        exec_line = match.group(1).strip() if match else ""
        if re.match(r"nocturne(\s|$)", exec_line) and "/home/" not in text:
            report.ok(f"desktop entry {d}: Exec={exec_line}")
        else:
            reason = "an absolute or host path" if "/home/" in text or exec_line.startswith("/") else "does not start with nocturne"
            report.fail(f"desktop entry {d}: Exec={exec_line!r} ({reason})")

    sizes = set()
    for r, p in rel.items():
        if "share/icons/" in r and r.endswith(".png") and not os.path.islink(p):
            dims = png_size(p)
            if dims and dims[0] == dims[1]:
                sizes.add(dims[0])
    missing = [s for s in ICON_SIZES if s not in sizes]
    if kind == "app":
        pass
    elif missing:
        report.fail(f"icons missing at {missing} (found {sorted(sizes)})")
    else:
        report.ok(f"icons at {list(ICON_SIZES)}")

    bad_paths = [r for r in rel if any(part in "/" + r for part in FORBIDDEN_PARTS)]
    if bad_paths:
        report.fail(f"forbidden path parts in: {bad_paths[:5]}")
    else:
        report.ok("no .agents, vendor/ or /home/ in any path")

    stamps = [r for r in rel if os.path.basename(r) == ".sha256"]
    if stamps:
        report.fail(f"fetch stamp files in the package: {stamps}")
    else:
        report.ok("no .sha256 stamp files")

    preview = [r for r in rel if os.path.basename(r).startswith(PREVIEW_BINARY)]
    for r, p in rel.items():
        if os.path.islink(p) or not os.path.isfile(p) or r in preview:
            continue
        if lemon_dir and r.startswith(lemon_dir + os.sep):
            continue
        with open(p, "rb") as handle:
            data = handle.read()
        if any(marker in data for marker in PREVIEW_MARKERS):
            preview.append(r)
    if preview:
        report.fail(f"the development-only real preview or the server is in the package: {preview[:5]}")
    else:
        report.ok("no development-only real preview or server in the package")

    leaks = []
    for r, p in rel.items():
        if os.path.islink(p) or not os.path.isfile(p):
            continue
        with open(p, "rb") as handle:
            data = handle.read()
        if is_text(data) and HOME_PATTERN.search(data):
            leaks.append(r)
    if leaks:
        report.fail(f"text files naming a home directory: {leaks[:5]}")
    else:
        report.ok("no text file names a home directory")

    if kind == "app":
        lemon_prefix = (lemon_dir + os.sep) if lemon_dir else None
        mac_leaks = []
        for r, p in rel.items():
            if os.path.islink(p) or not os.path.isfile(p) or (lemon_prefix and r.startswith(lemon_prefix)):
                continue
            with open(p, "rb") as handle:
                data = handle.read()
            if is_text(data) and MAC_HOME_PATTERN.search(data):
                mac_leaks.append(r)
        if mac_leaks:
            report.fail(f"text files naming a Mac home folder: {mac_leaks[:5]}")
        else:
            report.ok("no text file outside the engine names a Mac home folder")

    if builder_home:
        needle = builder_home.encode()
        lemon_prefix = (lemon_dir + os.sep) if lemon_dir else None
        found = []
        for r, p in rel.items():
            if os.path.islink(p) or not os.path.isfile(p):
                continue
            # The engine's files are verified byte for byte below and come from upstream.
            if lemon_prefix and r.startswith(lemon_prefix):
                continue
            with open(p, "rb") as handle:
                if needle in handle.read():
                    found.append(r)
        if found:
            report.fail(f"files carrying the builder's home directory {builder_home}: {found[:5]}")
        else:
            report.ok("no file outside the engine carries the builder's home directory")
    else:
        report.fail("no builder home given, so binaries were not scanned for it")


    if manifest and lemon_dir is not None:
        want = read_manifest(manifest)
        have, links = {}, []
        for r, p in rel.items():
            if not r.startswith(lemon_dir + os.sep):
                continue
            if os.path.islink(p):
                links.append(os.path.relpath(r, lemon_dir))
            elif os.path.isfile(p):
                have[os.path.relpath(r, lemon_dir)] = sha256(p)
        diffs = [f"symlink {name}" for name in sorted(links)]
        for name in sorted(set(want) | set(have)):
            if name not in have:
                diffs.append(f"missing {name}")
            elif name not in want:
                diffs.append(f"extra {name}")
            elif want[name] != have[name]:
                diffs.append(f"changed {name}")
        if diffs:
            report.fail(f"lemonade files differ from the verified manifest: {diffs[:5]}")
        else:
            report.ok(f"lemonade files match the verified manifest byte for byte ({len(want)} files)")
    elif manifest:
        report.fail("manifest given but no lemonade folder to compare")

    if kind == "appimage":
        check_bundled_libraries(root, rel, lemon_dir, sources, report)

    if kind in REQUIRED_DEPENDS:
        text = read_text(requires) if requires else ""
        missing = [name for name in REQUIRED_DEPENDS[kind] if name not in text]
        if missing:
            report.fail(f"{kind} does not declare: {missing}")
        else:
            report.ok(f"{kind} declares {', '.join(REQUIRED_DEPENDS[kind])}")


def check_app_bundle(root, rel, version, report):
    """Info.plist: identifier, executable, icon, version, minimum system and the microphone text."""
    info = rel.get(os.path.join("Contents", "Info.plist"))
    if not info:
        report.fail("no Contents/Info.plist")
        return
    try:
        with open(info, "rb") as handle:
            plist = plistlib.load(handle)
    except Exception as error:  # a damaged property list is the finding
        report.fail(f"Info.plist does not parse: {error}")
        return
    if plist.get("CFBundleIdentifier") == APP_IDENTIFIER:
        report.ok(f"bundle identifier {APP_IDENTIFIER}")
    else:
        report.fail(f"bundle identifier is {plist.get('CFBundleIdentifier')!r}, want {APP_IDENTIFIER}")
    exe = plist.get("CFBundleExecutable", "")
    if exe and os.path.join("Contents", "MacOS", exe) in rel:
        report.ok(f"CFBundleExecutable {exe} exists")
    else:
        report.fail(f"CFBundleExecutable {exe!r} is not in Contents/MacOS")
    icon = plist.get("CFBundleIconFile", "")
    icon = icon if icon.endswith(".icns") else icon + ".icns"
    if icon != ".icns" and os.path.join("Contents", "Resources", icon) in rel:
        report.ok(f"icon {icon} present")
    else:
        report.fail(f"CFBundleIconFile {plist.get('CFBundleIconFile')!r} is not in Contents/Resources")
    if version:
        got = plist.get("CFBundleShortVersionString")
        if got == version:
            report.ok(f"bundle version {got} equals the workspace version")
        else:
            report.fail(f"bundle version {got!r} differs from workspace version {version}")
    if re.match(r"^\d+(\.\d+)*$", str(plist.get("LSMinimumSystemVersion", ""))):
        report.ok(f"minimum macOS {plist['LSMinimumSystemVersion']}")
    else:
        report.fail("LSMinimumSystemVersion is missing")
    mic = str(plist.get("NSMicrophoneUsageDescription", "")).strip()
    if mic:
        report.ok(f"microphone sentence: {mic}")
    else:
        report.fail("NSMicrophoneUsageDescription is missing")


def parse_bundled(text):
    """Entries of BUNDLED-LIBRARIES.txt: name, licence, source package and files.

    An entry is `name version  (licence)`, its source package line, then its files up to the first
    line that is not one; the licence texts follow. Text that happens to look like an entry or a
    file is not read as one.
    """
    entries = []
    lines = text.splitlines()
    for i, line in enumerate(lines):
        head = re.match(r"^(\S+) (\S+)  \((.*)\)$", line)
        if not head or i + 1 >= len(lines) or not lines[i + 1].startswith("  source package: "):
            continue
        entry = {"name": head.group(1), "license": head.group(3), "srpm": lines[i + 1].split(": ", 1)[1].strip()}
        entry["files"] = []
        for later in lines[i + 2 :]:
            if not re.match(r"^  \S+\.so\S*$", later):
                break
            entry["files"].append(later.strip())
        entries.append(entry)
    return entries


def check_bundled_libraries(root, rel, lemon_dir, sources, report):
    libs = sorted(
        os.path.relpath(p, os.path.join(root, "usr", "lib"))
        for r, p in rel.items()
        if r.startswith(os.path.join("usr", "lib") + os.sep)
        and not (lemon_dir and r.startswith(lemon_dir + os.sep))
        and re.search(r"\.so(\.|$)", os.path.basename(r))
        and not os.path.isdir(p)
    )
    present = [
        pattern
        for pattern, _ in bundled_libs.EXCLUDED
        if any(fnmatch.fnmatch(lib, pattern) for lib in libs)
    ]
    if present:
        report.fail(f"modules the app does not use are still bundled: {present}")
    else:
        report.ok("none of the excluded modules is bundled")
    if not lemon_dir:
        return
    path = os.path.join(os.path.dirname(lemon_dir), "notices", "BUNDLED-LIBRARIES.txt")
    if path not in rel:
        return
    text = read_text(rel[path])
    entries = parse_bundled(text)
    listed = {f for e in entries for f in e["files"]}
    unlisted = [lib for lib in libs if lib not in listed]
    if unlisted:
        report.fail(f"bundled libraries missing from BUNDLED-LIBRARIES.txt: {unlisted[:5]}")
    else:
        report.ok(f"BUNDLED-LIBRARIES.txt lists all {len(libs)} bundled libraries")
    bad = []
    for e in entries:
        reason = bundled_libs.package_ok(e["name"], e["license"], e["files"])
        if reason:
            bad.append(f"{e['name']}: {reason}")
    if bad:
        report.fail(f"bundled libraries fail the copyleft policy: {bad[:3]}")
    else:
        report.ok(f"all {len(entries)} bundled library packages pass the copyleft policy")
    if sources:
        have = read_text(sources)
        missing = [e["srpm"] for e in entries if not e["srpm"] or e["srpm"] not in have]
        if missing:
            report.fail(f"source packages missing from the SOURCES manifest: {missing[:5]}")
        else:
            report.ok("the SOURCES manifest names every bundled source package")
    else:
        report.fail("no SOURCES manifest given for the bundled libraries")


def main(argv=None):
    parser = argparse.ArgumentParser()
    parser.add_argument("root")
    parser.add_argument("--version")
    parser.add_argument("--kind", default="tree", choices=["appimage", "deb", "rpm", "app", "tree"])
    parser.add_argument("--manifest")
    parser.add_argument("--requires")
    parser.add_argument("--package-version", help="version read from the package itself")
    parser.add_argument("--builder-home", help="the home folder of whoever built; must not appear in the package")
    parser.add_argument("--sources", help="the SOURCES manifest that ships beside the artefacts")
    args = parser.parse_args(argv)
    report = Report()
    if args.version and args.package_version:
        if args.version == args.package_version:
            report.ok(f"package version {args.package_version} equals the workspace version")
        else:
            report.fail(f"package version {args.package_version} differs from workspace version {args.version}")
    check(args.root, args.kind, args.manifest, args.requires, report, args.builder_home, args.sources, args.version)
    if report.failures:
        print(f"\n{len(report.failures)} check(s) failed")
        return 1
    print("\nall checks passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
