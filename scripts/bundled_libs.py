"""Licences, policy and sources for the system libraries an AppImage carries.

The AppImage copies shared libraries from the build computer. This module prunes the ones the
app does not need, takes each remaining library's licence and licence texts from the system
package database (rpm), applies the copyleft policy, and records the source package of each so
the corresponding source can be provided.

Policy: LGPL and permissive licences pass. A GPL or AGPL licence passes only through an OR
branch, a GCC or Bison exception, or an entry in ALLOW_GPL below (package plus library name, with
a one-line reason). A licence string that cannot be parsed fails unless LEGACY maps it.
"""

import glob
import os
import re
import subprocess

import notices

# Modules the app does not use. Removed from the AppDir before it is packed, so they are not
# shipped and not listed.
EXCLUDED = (
    ("gtk-3.0/modules/libpk-gtk-module.so", "PackageKit font prompt (GPL); the app installs nothing"),
    ("gtk-3.0/modules/libcanberra-gtk3-module.so", "event sounds module; the app plays none through GTK"),
    ("libcanberra-gtk3.so*", "only the event sounds module uses it"),
    ("libcanberra.so*", "only the event sounds module uses it"),
    ("gio/modules/libgvfsdbus.so", "network and trash file backends; the app opens local files only"),
    ("gio/modules/libgioremote-volume-monitor.so", "volume monitor backend; not used"),
    ("libgvfscommon.so*", "shared by the removed file backends"),
)

# (package, library name) -> why a GPL licence in the package does not bind the library. Fedora
# tags a package with every licence in its source tree (tools, docs, tests), so a GPL entry
# often belongs to something that is not shipped. Each entry was reviewed by hand.
ALLOW_GPL = {
    ("lcms2", "liblcms2.so.2"): "library is MIT; the GPL covers the command line tools",
    ("libffi", "libffi.so.8"): "library is MIT; the GPL with Texinfo exception covers the manual",
    ("glycin-libs", "libglycin-2.so.0"): "library is MPL-2.0 or LGPL-2.1-or-later; the GPL is one member of its vendored crate list; gdk-pixbuf needs it; review each release",
    ("colord-libs", "libcolord.so.2"): "library is LGPL-2.1-or-later; the GPL covers the daemon and tools",
    ("dbus-libs", "libdbus-1.so.3"): "library is AFL-2.1 or GPL-2.0-or-later, taken under AFL-2.1; the other GPL covers the daemon",
    ("dconf", "libdconfsettings.so"): "the GIO module is LGPL-2.1-or-later; the GPL covers the editor and service",
    ("flac-libs", "libFLAC.so.14"): "library is BSD-3-Clause; the GPL covers the flac tools",
    ("gnutls", "libgnutls.so.30"): "library is LGPL-2.1-or-later; the GPL-3.0-or-later covers the tools and the optional OpenSSL layer",
    ("javascriptcoregtk4.1", "libjavascriptcoregtk-4.1.so.0"): "WebKit is LGPL-2.1 and BSD; the GPL members are aggregate source tree entries, generated parsers under the Bison exception",
    ("webkit2gtk4.1", "libwebkit2gtk-4.1.so.0"): "WebKit is LGPL-2.1 and BSD; the GPL members are aggregate source tree entries, generated parsers under the Bison exception",
    ("keyutils-libs", "libkeyutils.so.1"): "library is LGPL-2.1-or-later; the GPL covers the keyctl tools",
    ("libatomic", "libatomic.so.1"): "GCC runtime library: GPL-3.0-or-later with the GCC runtime library exception",
    ("libgcrypt", "libgcrypt.so.20"): "library is LGPL-2.1-or-later; the GPL covers tests and tools",
    ("libidn2", "libidn2.so.0"): "library is GPL-2.0-or-later or LGPL-3.0-or-later, taken under the LGPL; the GPL-3.0-or-later covers the tool",
    ("libsndfile", "libsndfile.so.1"): "library is LGPL-2.1-or-later; the GPL covers examples and programs",
    ("libtasn1", "libtasn1.so.6"): "library is LGPL-2.1-or-later; the GPL covers the tools",
    ("mpg123-libs", "libmpg123.so.0"): "libmpg123 is LGPL-2.1 upstream; the distribution tags the whole source GPL-2.0-or-later. Owner to confirm",
    ("pcre2", "libpcre2-8.so.0"): "library is BSD-3-Clause; the GPL entries are build scripts under the Autoconf exception",
}

# Legacy licence strings, reviewed once and mapped to SPDX.
LEGACY = {
    "LGPLv2+": "LGPL-2.0-or-later",
    "LGPLv2": "LGPL-2.0-only",
    "LGPLv3+": "LGPL-3.0-or-later",
    "LGPLv3": "LGPL-3.0-only",
}

EXCEPTION_OK = re.compile(r"^(GCC|Bison)-exception")
SPDX_GPL = re.compile(r"^(?P<fam>A?GPL|LGPL)-\d\.\d(?:-only|-or-later|\+)?$")
GPL_LOOKING = re.compile(r"^(A?GPL|LGPL)", re.I)
NON_FREE = re.compile(r"proprietary|redistributable|noncommercial|-NC", re.I)
TEXT_NAME = re.compile(r"^(COPYING|LICENSE|LICENCE|COPYRIGHT|NOTICE|UNLICENSE)", re.I)


def prune(libdir):
    """Delete the excluded modules and libraries. Returns the relative paths removed.

    Fails when a library that stays still needs one that was removed.
    """
    removed = []
    for pattern, _ in EXCLUDED:
        for path in sorted(glob.glob(os.path.join(libdir, pattern))):
            os.remove(path)
            removed.append(os.path.relpath(path, libdir))
    gone = {os.path.basename(r) for r in removed}
    broken = []
    for path in glob.glob(os.path.join(libdir, "**", "*.so*"), recursive=True):
        if os.path.isdir(path):
            continue
        out = subprocess.run(["readelf", "-d", path], capture_output=True, text=True).stdout
        for needed in re.findall(r"\(NEEDED\)\s+Shared library: \[([^\]]+)\]", out):
            if needed in gone:
                broken.append(f"{os.path.relpath(path, libdir)} needs {needed}")
    if broken:
        raise notices.LicenceError("a removed library is still needed: " + "; ".join(broken))
    return removed


def normalise(token):
    if token in LEGACY:
        return LEGACY[token]
    if GPL_LOOKING.match(token) and not SPDX_GPL.match(token):
        raise notices.LicenceError(f"licence string {token!r} cannot be parsed and is not in the reviewed table")
    return token


def package_ok(package, expr, libs, allow=None):
    """Return a failure reason for a package, or None when its licence passes the policy."""
    allow = ALLOW_GPL if allow is None else allow
    covered = {lib for (name, lib) in allow if name == package}
    allowed_pkg = bool(covered) and all(os.path.basename(f) in covered for f in libs)

    def atom_ok(name):
        base, _, exc = name.partition(" WITH ")
        base = normalise(base)
        if NON_FREE.search(base):
            return False
        m = SPDX_GPL.match(base)
        if m and m.group("fam") in ("GPL", "AGPL"):
            if exc and EXCEPTION_OK.match(exc):
                return True
            return allowed_pkg
        return True

    try:
        ok = notices.evaluate(expr, atom_ok=atom_ok)
    except notices.LicenceError as err:
        return str(err)
    if ok:
        return None
    if covered:
        extra = sorted(f for f in libs if os.path.basename(f) not in covered)
        if extra:
            return f"GPL licence {expr!r}: allowlist for {package} does not cover {extra}"
    return f"GPL licence {expr!r} is not on the reviewed allowlist"


def rpm(*args):
    return subprocess.run(["rpm", *args], capture_output=True, text=True)


def ldconfig_index():
    index = {}
    proc = subprocess.run(["ldconfig", "-p"], capture_output=True, text=True)
    for line in proc.stdout.splitlines():
        m = re.match(r"\s*(\S+) \(.*\) => (\S+)", line)
        if m:
            index.setdefault(m.group(1), []).append(m.group(2))
    return index


def host_candidates(name, index):
    found = list(index.get(name, []))
    if not found:
        for root in ("/usr/lib64", "/usr/lib", "/usr/libexec"):
            found += glob.glob(os.path.join(root, "**", name), recursive=True)
    if not found and name.endswith(".so"):
        for root in ("/usr/lib64", "/usr/lib"):
            found += glob.glob(os.path.join(root, "**", name + ".*"), recursive=True)
    return found


def owner_of(path):
    proc = rpm("-qf", "--qf", "%{NAME}\t%{VERSION}-%{RELEASE}\t%{LICENSE}\t%{SOURCERPM}\n", path)
    if proc.returncode != 0 or not proc.stdout.strip():
        return None
    name, version, licence, srpm = proc.stdout.strip().splitlines()[0].split("\t", 3)
    nvr = re.sub(r"\.src\.rpm$", "", srpm)
    return {
        "name": name,
        "version": version,
        "license": licence,
        "srpm": nvr,
        "srpm_name": re.sub(r"-[^-]+-[^-]+$", "", nvr),
    }


def describe(libdir):
    """Map each bundled library to the system package it was copied from."""
    index = ldconfig_index()
    packages, unknown = {}, []
    for path in sorted(glob.glob(os.path.join(libdir, "**", "*.so*"), recursive=True)):
        if os.path.isdir(path):
            continue
        name = os.path.basename(path)
        owner = None
        for host in host_candidates(name, index):
            owner = owner_of(os.path.realpath(host))
            if owner:
                break
        rel = os.path.relpath(path, libdir)
        if owner is None:
            unknown.append(rel)
            continue
        entry = packages.setdefault(owner["name"], {**owner, "files": []})
        entry["files"].append(rel)
    return packages, unknown


def standard_text(expr):
    """A standard text for a package that installs none: an SPDX named file, else a template."""
    for token in notices.tokenize(expr):
        token = LEGACY.get(token, token)
        for path in sorted(glob.glob(f"/usr/share/licenses/*/{token}.txt")):
            return [(f"standard text {token}", notices.read_text(path))]
    for token in notices.tokenize(expr):
        if token in notices.TEMPLATES:
            return [(f"standard text {token}", notices.TEMPLATES[token])]
    return []


def licence_texts(package, srpm_name):
    """Licence files of a package, then of its sibling packages, then of the source name."""
    siblings = [package]
    listing = rpm("-qa", "--qf", "%{NAME}\t%{SOURCERPM}\n").stdout
    for line in listing.splitlines():
        name, _, srpm = line.partition("\t")
        if re.sub(r"-[^-]+-[^-]+\.src\.rpm$", "", srpm) == srpm_name and name not in siblings:
            siblings.append(name)
    found = []
    for name in siblings:
        found += [p for p in rpm("-qL", name).stdout.split("\n") if p]
        found += [p for p in rpm("-qd", name).stdout.split("\n") if p and TEXT_NAME.match(os.path.basename(p))]
        if found:
            break
    if not found:
        for root in ("/usr/share/licenses", "/usr/share/doc"):
            for p in glob.glob(os.path.join(root, srpm_name, "*")):
                if TEXT_NAME.match(os.path.basename(p)) or root.endswith("licenses"):
                    found.append(p)
    seen, out = set(), []
    for p in found:
        real = os.path.realpath(p)
        if os.path.isfile(real) and real not in seen:
            seen.add(real)
            out.append(real)
    return sorted(out)


def render(packages, texts):
    """The BUNDLED-LIBRARIES.txt document. `texts` maps package name to [(file name, body)]."""
    pool = notices.TextPool(texts.values())
    lines = [
        "Shared libraries carried inside the AppImage",
        "",
        "These come from the build computer's system packages and are loaded dynamically,",
        "so each one can be replaced by a different build of the same library. Each entry",
        "names the source package, from which the corresponding source can be obtained.",
        "Texts shipped identically by several packages are printed once, in the last section.",
        "",
    ]
    for name in sorted(packages):
        pkg = packages[name]
        lines += [f"{name} {pkg['version']}  ({pkg['license']})", f"  source package: {pkg['srpm']}"]
        lines += [f"  {f}" for f in sorted(pkg["files"])]
        reason = ALLOW_GPL.get((name, os.path.basename(sorted(pkg["files"])[0])))
        if reason:
            lines += [f"  reviewed: {reason}"]
        lines += pool.render(texts[name]) + [""]
    lines += pool.appendix("=" * 72, "Shared licence texts")
    return "\n".join(lines).rstrip("\n") + "\n"


def render_sources(packages):
    """One line per source package: NVR, the binary packages taken from it, and licences."""
    by_source = {}
    for pkg in packages.values():
        entry = by_source.setdefault(pkg["srpm"], {"names": [], "licenses": set()})
        entry["names"].append(pkg["name"])
        entry["licenses"].add(pkg["license"])
    lines = ["# Source packages of the libraries carried inside the AppImage", "# source package\tlibraries from it\tlicence"]
    for srpm in sorted(by_source):
        e = by_source[srpm]
        lines.append(f"{srpm}\t{','.join(sorted(e['names']))}\t{' | '.join(sorted(e['licenses']))}")
    return "\n".join(lines) + "\n"


def write(libdir, out, sources_out=None):
    packages, unknown = describe(libdir)
    if unknown:
        raise notices.LicenceError("bundled libraries with no owning system package: " + ", ".join(unknown))
    failures = []
    for name, pkg in sorted(packages.items()):
        reason = package_ok(name, pkg["license"], pkg["files"])
        if reason:
            failures.append(f"{name}: {reason}")
    texts = {}
    for name, pkg in packages.items():
        files = licence_texts(name, pkg["srpm_name"])
        texts[name] = [(os.path.basename(f), notices.read_text(f)) for f in files]
        if not files:
            texts[name] = standard_text(pkg["license"])
        if not texts[name]:
            failures.append(f"{name}: no licence text found on this computer")
    if failures:
        raise notices.LicenceError("bundled library policy failed:\n  " + "\n  ".join(failures))
    os.makedirs(out, exist_ok=True)
    target = os.path.join(out, "BUNDLED-LIBRARIES.txt")
    with open(target, "w", encoding="utf-8") as handle:
        handle.write(render(packages, texts))
    if sources_out:
        os.makedirs(os.path.dirname(os.path.abspath(sources_out)), exist_ok=True)
        with open(sources_out, "w", encoding="utf-8") as handle:
            handle.write(render_sources(packages))
    return target, packages
