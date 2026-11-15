"""Licences, policy and sources for the system libraries an AppImage carries.

The AppImage copies shared libraries from the build computer. This module prunes the ones the
app does not need, takes each remaining library's licence and licence texts from the system
package database (rpm on Fedora, dpkg on Debian and Ubuntu), applies the copyleft policy, and
records the source package of each so the corresponding source can be provided.

Policy: LGPL and permissive licences pass. A GPL or AGPL licence passes only through an OR
branch, a GCC or Bison exception, or an entry in ALLOW_GPL below (package plus library name, with
a one-line reason). A licence string that cannot be parsed fails unless LEGACY maps it.
"""

import fnmatch
import glob
import os
import re
import shutil
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
    # Debian and Ubuntu packages. Their licence is every licence the copyright file names, so the
    # GPL here is usually a tool, test or build script; the reason names which. A library name may
    # be a glob when one package ships many modules under the same licence.
    ("libblkid1", "libblkid.so.1"): "library is LGPL-2.1-or-later (libblkid/); the GPL covers the util-linux programs",
    ("libcolord2", "libcolord.so.2"): "library is LGPL-2.1-or-later (lib/colord/); the GPL covers the daemon, tools and the ColorHug library",
    ("libcurl3t64-gnutls", "libcurl-gnutls.so.4"): "library is curl; the GPL entries are build scripts under the Autoconf and Libtool exceptions",
    ("libdbus-1-3", "libdbus-1.so.3"): "library is AFL-2.1 or GPL-2.0-or-later, taken under AFL-2.1; the other GPL covers the tools",
    ("libdw1t64", "libdw.so.1"): "library is LGPL-3.0-or-later or GPL-2.0-or-later, taken under the LGPL; the GPL-3.0-or-later covers the elfutils tools",
    ("libelf1t64", "libelf.so.1"): "library is LGPL-3.0-or-later or GPL-2.0-or-later, taken under the LGPL; the GPL-3.0-or-later covers the elfutils tools",
    ("libenchant-2-2", "libenchant-2.so.2"): "library is LGPL-2.1-or-later; the GPL covers gnulib build snippets and the macOS spelling provider",
    ("libevdev2", "libevdev.so.2"): "library is MIT; the GPL covers copies of the kernel's input headers, interface definitions under the syscall note",
    ("libffi8", "libffi.so.8"): "library is MIT; the GPL covers the test suite and build scripts",
    ("libgcrypt20", "libgcrypt.so.20"): "library is LGPL-2.1-or-later; the GPL covers the manual and tools",
    ("libgdk-pixbuf-2.0-0", "libgdk_pixbuf-2.0.so*"): "library is LGPL-2.0-or-later; the GPL covers tests and the thumbnailer",
    ("libgdk-pixbuf-2.0-0", "libpixbufloader-*.so"): "loaders are LGPL-2.0-or-later; the GPL covers tests and the thumbnailer",
    ("libglib2.0-0t64", "libgio-2.0.so*"): "library is LGPL-2.1-or-later; the GPL covers generator scripts and glib-gettextize",
    ("libglib2.0-0t64", "libglib-2.0.so*"): "library is LGPL-2.1-or-later; the GPL covers generator scripts and glib-gettextize",
    ("libglib2.0-0t64", "libgmodule-2.0.so*"): "library is LGPL-2.1-or-later; the GPL covers generator scripts and glib-gettextize",
    ("libglib2.0-0t64", "libgobject-2.0.so*"): "library is LGPL-2.1-or-later; the GPL covers generator scripts and glib-gettextize",
    ("libgnutls30t64", "libgnutls.so.30"): "library is LGPL-2.1-or-later; the GPL-3.0-or-later covers the tools and the optional OpenSSL layer",
    ("libharfbuzz-icu0", "libharfbuzz-icu.so.0"): "library is MIT; the GPL covers test fonts, gtk-doc files and build scripts under the Autoconf and Libtool exceptions",
    ("libhogweed6t64", "libhogweed.so.6"): "library is LGPL-3.0-or-later or GPL-2.0-or-later, taken under the LGPL; the GPL covers the tools' getopt and build scripts",
    ("libidn2-0", "libidn2.so.0"): "library is GPL-2.0-or-later or LGPL-3.0-or-later, taken under the LGPL; the GPL-3.0-or-later covers the tool and gnulib",
    ("libjavascriptcoregtk-4.1-0", "libjavascriptcoregtk-4.1.so.0"): "WebKit is LGPL-2.1 and BSD; the GPL members are generated parsers under the Bison exception and ANGLE's parser generator skeletons",
    ("libwebkit2gtk-4.1-0", "libwebkit2gtk-4.1.so.0"): "WebKit is LGPL-2.1 and BSD; the GPL members are generated parsers under the Bison exception and ANGLE's parser generator skeletons",
    ("libwebkit2gtk-4.1-0", "libwebkit2gtkinjectedbundle.so"): "WebKit is LGPL-2.1 and BSD; the GPL members are generated parsers under the Bison exception and ANGLE's parser generator skeletons",
    ("libkeyutils1", "libkeyutils.so.1"): "library is LGPL-2.0-or-later (keyutils.c); the GPL covers the keyctl tools",
    ("liblcms2-2", "liblcms2.so.2"): "library is MIT; the GPL-3.0 covers the fast_float and threaded plugins, separate libraries that are not bundled",
    ("libldap2", "libldap.so.2"): "library is OpenLDAP-2.8; the GPL entries are build scripts under the Autoconf and Libtool exceptions",
    ("libldap2", "liblber.so.2"): "library is OpenLDAP-2.8; the GPL entries are build scripts under the Autoconf and Libtool exceptions",
    ("liblz4-1", "liblz4.so.1"): "library is BSD-2-Clause; the GPL covers the lz4 programs and tests",
    ("liblzma5", "liblzma.so.5"): "library is public domain; the GPL covers the xz scripts, the tools' getopt and the Doxygen file",
    ("libmount1", "libmount.so.1"): "library is LGPL-2.1-or-later (libmount/); the GPL covers the util-linux programs",
    ("libnettle8t64", "libnettle.so.8"): "library is LGPL-3.0-or-later or GPL-2.0-or-later, taken under the LGPL; the GPL covers the tools' getopt and build scripts",
    ("libnghttp2-14", "libnghttp2.so.14"): "library is MIT; the GPL covers one Autoconf macro under the Autoconf exception",
    ("libp11-kit0", "libp11-kit.so.0"): "library is BSD-3-Clause; the GPL entries are build scripts under the Autoconf exception",
    ("libproxy1v5", "libproxy.so.1"): "library is LGPL-2.1-or-later; the GPL covers a code style script",
    ("libproxy1v5", "libpxbackend-1.0.so"): "library is LGPL-2.1-or-later; the GPL covers a code style script",
    ("libsecret-1-0", "libsecret-1.so.0"): "library is LGPL-2.1-or-later; the GPL covers a Python test mock",
    ("libselinux1", "libselinux.so.1"): "library is public domain; the GPL covers the avcstat utility",
    ("libsystemd0", "libsystemd.so.0"): "library is LGPL-2.1-or-later; the GPL covers udev helper programs and the kernel headers under the syscall note",
    ("libtasn1-6", "libtasn1.so.6"): "library is LGPL-2.1-or-later; the GPL covers the tools",
    ("libudev1", "libudev.so.1"): "library is LGPL-2.1-or-later; the GPL covers udev helper programs and the kernel headers under the syscall note",
    ("libunistring5", "libunistring.so.5"): "library is LGPL-3.0-or-later or GPL-2.0-or-later, taken under the LGPL; the GPL covers tests, documentation and build scripts",
}

# Legacy licence strings, reviewed once and mapped to SPDX. The second group are the names
# Debian copyright files use, exactly as they appear there; an unversioned LGPL lets the
# recipient choose any version, so it reads as 2.0 or later. Any name with GPL in it must be
# spelled in SPDX or be listed here. CUPS's exception only adds permission to link GPL code.
LEGACY = {
    "LGPLv2+": "LGPL-2.0-or-later",
    "LGPLv2": "LGPL-2.0-only",
    "LGPLv3+": "LGPL-3.0-or-later",
    "LGPLv3": "LGPL-3.0-only",
    "GPL-1+": "GPL-1.0-or-later",
    "GPL-2": "GPL-2.0-only",
    "GPL-2+": "GPL-2.0-or-later",
    "GPL-3": "GPL-3.0-only",
    "GPL-3+": "GPL-3.0-or-later",
    "LGPL": "LGPL-2.0-or-later",
    "LGPL-2": "LGPL-2.0-only",
    "LGPL-2+": "LGPL-2.0-or-later",
    "LGPL-2.1+~OpenSSL": "LGPL-2.1-or-later",
    "LGPL-3+": "LGPL-3.0-or-later",
    "Apache-2.0-with-GPL2-LGPL2-Exception": "Apache-2.0",
}

# Debian binary packages whose copyright file does not name its licences in a form that parses:
# the licence tag of the same library's package in Fedora, which the rpm path reads, checked
# against the Debian copyright text. Where the two differ the Debian text wins (librtmp1), and
# libatomic1 is the one runtime library taken from gcc-14. Each use is printed and recorded in
# BUNDLED-LIBRARIES.txt.
FEDORA_KRB5 = (
    "Brian-Gladman-2-Clause AND BSD-2-Clause AND (BSD-2-Clause OR GPL-2.0-or-later) AND BSD-2-Clause-first-lines"
    " AND BSD-3-Clause AND BSD-4-Clause AND CMU-Mach-nodoc AND FSFULLRWD AND HPND AND HPND-export2-US"
    " AND HPND-export-US AND HPND-export-US-acknowledgement AND HPND-export-US-modify AND ISC AND MIT"
    " AND MIT-CMU AND OLDAP-2.8 AND OpenVision"
)
DEBIAN_LICENCES = {
    "libatomic1": "GPL-3.0-or-later WITH GCC-exception-3.1",
    "libavahi-client3": "LGPL-2.1-or-later AND LGPL-2.0-or-later AND BSD-2-Clause-Views AND MIT",
    "libavahi-common3": "LGPL-2.1-or-later AND LGPL-2.0-or-later AND BSD-2-Clause-Views AND MIT",
    "libcairo-gobject2": "LGPL-2.1-only OR MPL-1.1",
    "libcairo2": "LGPL-2.1-only OR MPL-1.1",
    "libdrm-amdgpu1": "MIT",
    "libgcrypt20": (
        "BSD-3-Clause AND (BSD-3-Clause OR GPL-2.0-only) AND GPL-2.0-or-later AND LGPL-2.1-or-later"
        " AND LGPL-2.0-or-later AND MIT-Modern-Variant"
    ),
    "libgnutls30t64": "GPL-3.0-or-later AND LGPL-2.1-or-later",
    "libgssapi-krb5-2": FEDORA_KRB5,
    "libgstreamer-gl1.0-0": "LGPL-2.1-or-later",
    "libgstreamer-plugins-base1.0-0": "LGPL-2.1-or-later",
    "libgstreamer1.0-0": "LGPL-2.1-or-later",
    "libk5crypto3": FEDORA_KRB5,
    "libkrb5-3": FEDORA_KRB5,
    "libkrb5support0": FEDORA_KRB5,
    "liborc-0.4-0t64": "BSD-2-Clause AND BSD-3-Clause",
    "libpixman-1-0": "MIT",
    "librtmp1": "LGPL-2.1-or-later",
    "libtasn1-6": "GPL-3.0-or-later AND LGPL-2.1-or-later",
    "libwayland-cursor0": "MIT",
    "libwayland-egl1": "MIT",
    "libwayland-server0": "MIT",
    "libxau6": "MIT-open-group",
    "libxcb-render0": "X11",
    "libxcb-shm0": "X11",
    "libxcomposite1": "MIT AND HPND-sell-variant",
    "libxcursor1": "HPND-sell-variant",
    "libxdamage1": "HPND-sell-variant",
    "libxdmcp6": "MIT-open-group",
    "libxext6": "MIT-open-group AND X11 AND HPND AND HPND-sell-variant AND SMLNJ AND MIT AND ISC AND HPND-doc AND HPND-doc-sell",
    "libxfixes3": "MIT AND HPND-sell-variant",
    "libxi6": "MIT-open-group AND SMLNJ AND MIT",
    "libxinerama1": "MIT AND MIT-open-group AND X11",
    "libxkbcommon0": "MIT AND X11 AND MIT-CMU",
    "libxrandr2": "HPND-sell-variant",
    "libxrender1": "HPND-sell-variant",
    "libxslt1.1": "MIT",
}

EXCEPTION_OK = re.compile(r"^(GCC|Bison)-exception")
SPDX_GPL = re.compile(r"^(?P<fam>A?GPL|LGPL)-\d\.\d(?:-only|-or-later|\+)?$")
GPL_LOOKING = re.compile(r"GPL", re.I)
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
    if GPL_LOOKING.search(token) and not SPDX_GPL.match(token):
        raise notices.LicenceError(f"licence string {token!r} cannot be parsed and is not in the reviewed table")
    return token


def allowed_reason(package, lib, allow=None):
    """The allowlist reason for a library of a package; with lib None, any entry of the package."""
    for (name, pattern), reason in (ALLOW_GPL if allow is None else allow).items():
        if name == package and (lib is None or fnmatch.fnmatch(lib, pattern)):
            return reason
    return None


def package_ok(package, expr, libs, allow=None):
    """Return a failure reason for a package, or None when its licence passes the policy."""
    allow = ALLOW_GPL if allow is None else allow
    covered = bool(allowed_reason(package, None, allow))
    allowed_pkg = covered and all(allowed_reason(package, os.path.basename(f), allow) for f in libs)

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
        extra = sorted(f for f in libs if not allowed_reason(package, os.path.basename(f), allow))
        if extra:
            return f"GPL licence {expr!r}: allowlist for {package} does not cover {extra}"
    return f"GPL licence {expr!r} is not on the reviewed allowlist"


# Every query is bounded, and none reads standard input: a database that does not answer fails
# the step with its name instead of holding it until the job times out.
QUERY_TIMEOUT = 120


def run(cmd):
    try:
        return subprocess.run(
            cmd, capture_output=True, text=True, stdin=subprocess.DEVNULL, timeout=QUERY_TIMEOUT
        )
    except subprocess.TimeoutExpired:
        raise notices.LicenceError(f"{cmd[0]} {cmd[1]} gave no answer in {QUERY_TIMEOUT} seconds") from None


def ldconfig_index():
    index = {}
    for line in run(["ldconfig", "-p"]).stdout.splitlines():
        m = re.match(r"\s*(\S+) \(.*\) => (\S+)", line)
        if m:
            index.setdefault(m.group(1), []).append(m.group(2))
    return index


class HostFiles:
    """Where a bundled library came from on this computer.

    The linker cache answers for most libraries. A module (GIO, GTK, a pixbuf loader) is looked up
    by its path relative to each library folder, then by name in one walk of those folders. The
    walk runs at most once and never follows a symlink, so its cost does not grow with the number
    of libraries or with what else is installed under a symlinked folder.
    """

    ROOTS = ("/usr/lib64", "/usr/lib", "/usr/libexec")

    def __init__(self, index=None):
        self.index = ldconfig_index() if index is None else index
        dirs = []
        for path in [p for paths in self.index.values() for p in paths]:
            if os.path.dirname(path) not in dirs:
                dirs.append(os.path.dirname(path))
        self.dirs = [d for d in dirs + [r for r in self.ROOTS if r not in dirs] if os.path.isdir(d)]
        self.by_name = None

    def candidates(self, rel):
        name = os.path.basename(rel)
        found = list(self.index.get(name, []))
        found += [os.path.join(d, rel) for d in self.dirs if os.path.isfile(os.path.join(d, rel))]
        if not found:
            found = self.walk().get(name, [])
        if not found and name.endswith(".so"):
            found = [p for n, paths in sorted(self.walk().items()) if n.startswith(name + ".") for p in paths]
        unique = []
        for path in found:
            if os.path.realpath(path) not in unique:
                unique.append(os.path.realpath(path))
        return unique

    def walk(self):
        if self.by_name is None:
            self.by_name, seen = {}, set()
            for root in self.dirs:
                for base, dirs, names in os.walk(root):
                    if os.path.realpath(base) in seen:
                        dirs[:] = []
                        continue
                    seen.add(os.path.realpath(base))
                    for name in names:
                        if ".so" in name:
                            self.by_name.setdefault(name, []).append(os.path.join(base, name))
        return self.by_name


class Rpm:
    """An rpm build computer (Fedora): licence tags, licence files and source rpms."""

    name = tool = "rpm"

    def __init__(self):
        self.listing = None

    def owners(self, path):
        proc = run(["rpm", "-qf", "--qf", "%{NAME}\t%{VERSION}-%{RELEASE}\t%{LICENSE}\t%{SOURCERPM}\n", path])
        if proc.returncode != 0 or not proc.stdout.strip():
            return None
        name, version, licence, srpm = proc.stdout.strip().splitlines()[0].split("\t", 3)
        nvr = re.sub(r"\.src\.rpm$", "", srpm)
        return {
            "name": name,
            "version": version,
            "license": licence,
            "source": nvr,
            "source_name": re.sub(r"-[^-]+-[^-]+$", "", nvr),
        }

    def licence_texts(self, pkg):
        """Licence files of a package, then of its sibling packages, then of the source name."""
        package, srpm_name = pkg["name"], pkg["source_name"]
        if self.listing is None:
            self.listing = run(["rpm", "-qa", "--qf", "%{NAME}\t%{SOURCERPM}\n"]).stdout
        siblings = [package]
        for line in self.listing.splitlines():
            name, _, srpm = line.partition("\t")
            if re.sub(r"-[^-]+-[^-]+\.src\.rpm$", "", srpm) == srpm_name and name not in siblings:
                siblings.append(name)
        found = []
        for name in siblings:
            found += [p for p in run(["rpm", "-qL", name]).stdout.split("\n") if p]
            found += [
                p for p in run(["rpm", "-qd", name]).stdout.split("\n") if p and TEXT_NAME.match(os.path.basename(p))
            ]
            if found:
                break
        if not found:
            for root in ("/usr/share/licenses", "/usr/share/doc"):
                for p in glob.glob(os.path.join(root, srpm_name, "*")):
                    if TEXT_NAME.match(os.path.basename(p)) or root.endswith("licenses"):
                        found.append(p)
        return existing(found)


class Dpkg:
    """A Debian or Ubuntu build computer: dpkg owners, source packages and copyright files.

    The licence is read from the package's machine-readable copyright file: every licence its
    Files paragraphs name, joined with AND, as Fedora tags a package with every licence in its
    source tree. Paragraphs that cover only debian/ (the packaging, never built into a library)
    are left out.
    """

    name = tool = "dpkg"
    FIELDS = "${Package}\t${Version}\t${source:Package}\t${source:Version}\n"

    def __init__(self, doc_root="/usr/share/doc", licences_root="/usr/share/common-licenses"):
        self.doc_root = doc_root
        self.licences_root = licences_root
        self.packages = {}

    def owners(self, path):
        proc = run(["dpkg-query", "-S", "--", *usr_aliases(path)])
        for line in proc.stdout.splitlines():
            m = None if "diversion " in line else re.match(r"^(.+?): (/.*)$", line)
            if m:
                return self.package(m.group(1).split(", ")[0])
        return None

    def package(self, query):
        if query not in self.packages:
            proc = run(["dpkg-query", "-W", "-f", self.FIELDS, query])
            if proc.returncode != 0 or not proc.stdout.strip():
                raise notices.LicenceError(f"dpkg-query knows no package {query!r}: {proc.stderr.strip()}")
            name, version, source, source_version = proc.stdout.splitlines()[0].split("\t")
            source = source or name
            source_version = source_version or version
            copyright_file = os.path.realpath(os.path.join(self.doc_root, name, "copyright"))
            text = notices.read_text(copyright_file) if os.path.isfile(copyright_file) else ""
            self.packages[query] = {
                "name": name,
                "version": version,
                "license": copyright_licence(text) or DEBIAN_LICENCES.get(name),
                "reviewed_licence": copyright_licence(text) is None and name in DEBIAN_LICENCES,
                "source": f"{source}_{source_version}",
                "source_name": source,
                "copyright": copyright_file if text else None,
                "copyright_text": text,
            }
        return self.packages[query]

    def licence_texts(self, pkg):
        """The copyright file, then each common licence text it refers to."""
        if not pkg["copyright"]:
            return []
        found = [pkg["copyright"]]
        for name in re.findall(re.escape(self.licences_root) + r"/([\w.+-]*\w)", pkg["copyright_text"]):
            found.append(os.path.join(self.licences_root, name))
        return existing(found)


def usr_aliases(path):
    """A path and its other spelling across the merged /usr: dpkg may record either."""
    aliases = [path]
    for short in ("/lib", "/lib64", "/bin", "/sbin"):
        if path.startswith("/usr" + short + "/"):
            aliases.append(path[len("/usr"):])
        elif path.startswith(short + "/"):
            aliases.append("/usr" + path)
    return aliases


def existing(paths):
    seen, out = set(), []
    for p in paths:
        real = os.path.realpath(p)
        if os.path.isfile(real) and real not in seen:
            seen.add(real)
            out.append(real)
    return sorted(out)


def paragraphs(text):
    """The paragraphs of a Debian control-style file, as dicts of field to value."""
    out = []
    for block in re.split(r"\n[ \t]*\n", text.strip("\n")):
        fields, key = {}, None
        for line in block.splitlines():
            if line[:1] in (" ", "\t") and key:
                fields[key] += "\n" + line.strip()
            elif ":" in line and not line.startswith("#"):
                key, _, value = line.partition(":")
                key = key.strip()
                fields[key] = value.strip()
        if fields:
            out.append(fields)
    return out


def debian_expression(field):
    """A Debian copyright License line as an expression notices.evaluate parses.

    `X with Y exception` becomes `X WITH Y-exception`; a comma closes a group, so
    `A or B, and C` becomes `(A OR B) AND (C)`. Licence names are kept as Debian spells them.
    """

    def part(text):
        text = re.sub(
            r"\s+with\s+(.+?)\s+exception\b",
            lambda m: " WITH " + "-".join(m.group(1).split()) + "-exception",
            text.strip(),
            flags=re.I,
        )
        return re.sub(r"(?<=\s)(and|or|with)(?=\s)", lambda m: m.group(1).upper(), text, flags=re.I)

    pieces = [p.strip() for p in field.split(",") if p.strip()]
    if not pieces:
        return None
    expr = part(pieces[0])
    for piece in pieces[1:]:
        m = re.match(r"^(and|or)\s+(.*)$", piece, re.I)
        op, rest = (m.group(1).upper(), m.group(2)) if m else ("AND", piece)
        expr = f"({expr}) {op} ({part(rest)})"
    return expr


def copyright_licence(text):
    """Every licence a machine-readable copyright file names, joined with AND.

    None when the file is not in the machine-readable format or its licence names do not parse
    (free text such as `BSD (3 clause)`).
    """
    paras = paragraphs(text)
    if not paras or not re.search(r"copyright-format/1\.0", paras[0].get("Format", "")):
        return None
    found = []
    for para in paras:
        if "Files" not in para or "License" not in para:
            continue
        if all(f.startswith("debian/") for f in para["Files"].split()):
            continue
        expr = debian_expression(para["License"].split("\n", 1)[0])
        if expr is None:
            return None
        if expr not in found:
            found.append(expr)
    if not found and paras[0].get("License"):
        found.append(debian_expression(paras[0]["License"].split("\n", 1)[0]))
    if not found or None in found:
        return None
    expr = found[0] if len(found) == 1 else " AND ".join(f"({e})" if re.search(r"\s(OR|AND)\s", e) else e for e in sorted(found))
    try:
        notices.evaluate(expr, atom_ok=lambda name: True)
    except notices.LicenceError:
        return None
    return expr


def package_databases():
    """The package databases that know this computer's C library, rpm first.

    An Ubuntu runner has rpm (to build the rpm) with an empty database; a Fedora computer may
    have dpkg the same way. Neither knowing the C library means the bundled libraries' licences
    cannot be read here at all, which fails at once.
    """
    index = ldconfig_index()
    libc = sorted({os.path.realpath(p) for p in index.get("libc.so.6", [])})
    found = []
    for db in (Rpm(), Dpkg()):
        if shutil.which(db.tool) and any(db.owners(p) for p in libc):
            found.append(db)
    if not found:
        raise notices.LicenceError(
            "neither the rpm nor the dpkg database knows this computer's C library "
            f"({', '.join(libc) or 'not in the linker cache'}); the bundled libraries' licences and "
            "sources are read from the system packages, so build where they were installed by rpm or dpkg"
        )
    return found, index


def describe(libdir, databases=None, host=None, log=None):
    """Map each bundled library to the system package it was copied from.

    Prints one line per library, with its owner, as it goes.
    """
    if databases is None:
        databases, index = package_databases()
        host = host or HostFiles(index)
    host = host or HostFiles()
    log = log or (lambda line: print(line, flush=True))
    libs = sorted(p for p in glob.glob(os.path.join(libdir, "**", "*.so*"), recursive=True) if not os.path.isdir(p))
    log(f"bundled libraries: {len(libs)}, package database: {', '.join(db.name for db in databases)}")
    packages, unknown = {}, []
    for path in libs:
        rel = os.path.relpath(path, libdir)
        owner = None
        for candidate in host.candidates(rel):
            for db in databases:
                owner = db.owners(candidate)
                if owner:
                    owner = {**owner, "database": db}
                    break
            if owner:
                break
        if owner is None:
            log(f"  {rel}: no system package owns it")
            unknown.append(rel)
            continue
        note = ", licence from DEBIAN_LICENCES" if owner.get("reviewed_licence") else ""
        log(f"  {rel}: {owner['name']} ({owner['database'].name}{note})")
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
        lines += [f"{name} {pkg['version']}  ({pkg['license']})", f"  source package: {pkg['source']}"]
        lines += [f"  {f}" for f in sorted(pkg["files"])]
        reason = allowed_reason(name, os.path.basename(sorted(pkg["files"])[0]))
        if reason:
            lines += [f"  reviewed: {reason}"]
        if pkg.get("reviewed_licence"):
            lines += ["  licence: reviewed by hand; the copyright file does not name it in machine-readable form"]
        lines += pool.render(texts[name]) + [""]
    lines += pool.appendix("=" * 72, "Shared licence texts")
    return "\n".join(lines).rstrip("\n") + "\n"


def render_sources(packages):
    """One line per source package: NVR, the binary packages taken from it, and licences."""
    by_source = {}
    for pkg in packages.values():
        entry = by_source.setdefault(pkg["source"], {"names": [], "licenses": set()})
        entry["names"].append(pkg["name"])
        entry["licenses"].add(pkg["license"])
    lines = ["# Source packages of the libraries carried inside the AppImage", "# source package\tlibraries from it\tlicence"]
    for source in sorted(by_source):
        e = by_source[source]
        lines.append(f"{source}\t{','.join(sorted(e['names']))}\t{' | '.join(sorted(e['licenses']))}")
    return "\n".join(lines) + "\n"


def write(libdir, out, sources_out=None):
    packages, unknown = describe(libdir)
    if unknown:
        raise notices.LicenceError("bundled libraries with no owning system package: " + ", ".join(unknown))
    failures = []
    for name, pkg in sorted(packages.items()):
        if pkg["license"] is None:
            failures.append(
                f"{name}: {pkg.get('copyright') or 'its copyright file'} does not name its licences in the"
                f" machine-readable format and {name} is not in DEBIAN_LICENCES"
            )
            continue
        reason = package_ok(name, pkg["license"], pkg["files"])
        if reason:
            failures.append(f"{name}: {reason}")
    texts = {}
    for name, pkg in packages.items():
        files = pkg["database"].licence_texts(pkg)
        texts[name] = [(os.path.basename(f), notices.read_text(f)) for f in files]
        if not files:
            texts[name] = standard_text(pkg["license"] or "")
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
