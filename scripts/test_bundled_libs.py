"""Tests for scripts/bundled_libs.py: the copyleft policy, pruning and the documents."""

import os
import shutil
import subprocess
import tempfile
import unittest
from unittest import mock

import bundled_libs as b
import notices


class Policy(unittest.TestCase):
    def test_permissive_and_lgpl_pass(self):
        self.assertIsNone(b.package_ok("p", "MIT AND LGPL-2.1-or-later AND BSD-3-Clause", ["libp.so.1"]))

    def test_unlisted_gpl_fails(self):
        reason = b.package_ok("p", "LGPL-2.1-or-later AND GPL-2.0-or-later", ["libp.so.1"])
        self.assertIn("not on the reviewed allowlist", reason)

    def test_gpl_alone_fails_even_with_an_or_that_is_also_gpl(self):
        self.assertIsNotNone(b.package_ok("p", "GPL-2.0-only OR GPL-3.0-only", ["libp.so.1"]))

    def test_or_branch_with_a_permissive_side_passes(self):
        self.assertIsNone(b.package_ok("p", "GPL-2.0-or-later OR MIT", ["libp.so.1"]))

    def test_gcc_and_bison_exceptions_pass(self):
        self.assertIsNone(b.package_ok("p", "GPL-3.0-or-later WITH GCC-exception-3.1", ["libp.so.1"]))
        self.assertIsNone(b.package_ok("p", "GPL-3.0-or-later WITH Bison-exception-2.2", ["libp.so.1"]))
        self.assertIsNotNone(b.package_ok("p", "GPL-3.0-or-later WITH Texinfo-exception", ["libp.so.1"]))

    def test_allowlist_is_keyed_by_package_and_library(self):
        allow = {("p", "libp.so.1"): "reason"}
        self.assertIsNone(b.package_ok("p", "GPL-2.0-or-later", ["libp.so.1"], allow))
        self.assertIsNotNone(b.package_ok("q", "GPL-2.0-or-later", ["libp.so.1"], allow))
        extra = b.package_ok("p", "GPL-2.0-or-later", ["libp.so.1", "libother.so.2"], allow)
        self.assertIn("libother.so.2", extra)

    def test_unparseable_legacy_string_fails_unless_mapped(self):
        self.assertIn("cannot be parsed", b.package_ok("p", "GPLv2+", ["libp.so.1"]))
        self.assertIsNone(b.package_ok("p", "LGPLv2+", ["libp.so.1"]))

    def test_non_free_licence_fails(self):
        self.assertIsNotNone(b.package_ok("p", "LicenseRef-Proprietary", ["libp.so.1"]))

    def test_an_allowlist_entry_may_name_modules_by_glob(self):
        allow = {("p", "libmod-*.so"): "reason"}
        self.assertIsNone(b.package_ok("p", "GPL-2.0-or-later", ["a/libmod-x.so", "libmod-y.so"], allow))
        self.assertIn("libother.so.1", b.package_ok("p", "GPL-2.0-or-later", ["libmod-x.so", "libother.so.1"], allow))

    def test_every_allowlist_entry_has_a_reason(self):
        for key, reason in b.ALLOW_GPL.items():
            self.assertTrue(reason.strip(), key)


class Pruning(unittest.TestCase):
    def test_excluded_modules_are_removed_and_dependents_checked(self):
        tmp = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, tmp, True)
        for rel in ("gtk-3.0/modules/libpk-gtk-module.so", "libcanberra.so.0", "libkeep.so.1"):
            path = os.path.join(tmp, rel)
            os.makedirs(os.path.dirname(path), exist_ok=True)
            with open(path, "wb") as handle:
                handle.write(b"not an elf")
        removed = b.prune(tmp)
        self.assertIn("gtk-3.0/modules/libpk-gtk-module.so", removed)
        self.assertIn("libcanberra.so.0", removed)
        self.assertTrue(os.path.exists(os.path.join(tmp, "libkeep.so.1")))

    def test_a_removed_pixbuf_loader_leaves_the_loader_cache(self):
        tmp = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, tmp, True)
        loaders = os.path.join(tmp, "gdk-pixbuf-2.0", "2.10.0", "loaders")
        os.makedirs(loaders)
        for rel in ("libpixbufloader-png.so", "libpixbufloader-tiff.so"):
            with open(os.path.join(loaders, rel), "wb") as handle:
                handle.write(b"not an elf")
        cache = os.path.join(tmp, "gdk-pixbuf-2.0", "2.10.0", "loaders.cache")
        with open(cache, "w") as handle:
            handle.write(
                "# GdkPixbuf Image Loader Modules file\n#\n"
                '"libpixbufloader-tiff.so"\n"tiff" 5 "gdk-pixbuf" "TIFF" "LGPL"\n"image/tiff" ""\n\n'
                '"libpixbufloader-png.so"\n"png" 5 "gdk-pixbuf" "PNG" "LGPL"\n"image/png" ""\n\n'
            )
        removed = b.prune(tmp)
        self.assertEqual(removed, ["gdk-pixbuf-2.0/2.10.0/loaders/libpixbufloader-tiff.so"])
        with open(cache) as handle:
            text = handle.read()
        self.assertNotIn("tiff", text)
        self.assertIn('"libpixbufloader-png.so"', text)
        self.assertTrue(text.startswith("# GdkPixbuf Image Loader Modules file"))


class Documents(unittest.TestCase):
    PKGS = {
        "libx": {"name": "libx", "version": "1-1", "license": "MIT", "source": "x-1-1.fc44", "files": ["libx.so.1"]},
        "liby": {"name": "liby", "version": "2-1", "license": "LGPL-2.1-or-later", "source": "y-2-1.fc44", "files": ["liby.so.2"]},
    }

    def test_document_names_sources_and_texts(self):
        texts = {"libx": [("COPYING", "MIT TEXT")], "liby": [("COPYING", "LGPL TEXT")]}
        out = b.render(self.PKGS, texts)
        for needle in ("source package: x-1-1.fc44", "MIT TEXT", "LGPL TEXT", "libx.so.1"):
            self.assertIn(needle, out)

    def test_sources_manifest_has_one_line_per_source_package(self):
        lines = b.render_sources(self.PKGS).splitlines()
        self.assertEqual(len([line for line in lines if not line.startswith("#")]), 2)
        self.assertTrue(any(line.startswith("x-1-1.fc44\tlibx\t") for line in lines))

    def test_a_package_without_any_text_is_reported(self):
        self.assertEqual(b.standard_text("NoSuchLicence-9.9"), [])
        self.assertTrue(b.standard_text("MIT"))


def proc(stdout="", returncode=0, stderr=""):
    return subprocess.CompletedProcess([], returncode, stdout, stderr)


class Recorded:
    """Answers commands from recorded output, keyed by the command line; unknown ones fail."""

    def __init__(self, answers):
        self.answers = answers
        self.calls = []

    def __call__(self, cmd):
        self.calls.append(cmd)
        return self.answers.get(tuple(cmd), proc(returncode=1))


# Recorded on Ubuntu 24.04 (dpkg 1.22.6). dpkg-query -S exits 1 when one of the two spellings
# is not recorded, and still prints the owner of the other.
GLIB = "/usr/lib/x86_64-linux-gnu/libglib-2.0.so.0.8000.0"
DPKG_S = proc(
    "libglib2.0-0t64:amd64: /usr/lib/x86_64-linux-gnu/libglib-2.0.so.0.8000.0\n",
    1,
    "dpkg-query: no path found matching pattern /lib/x86_64-linux-gnu/libglib-2.0.so.0.8000.0\n",
)
DPKG_W = proc("libglib2.0-0t64\t2.80.0-6ubuntu3.10\tglib2.0\t2.80.0-6ubuntu3.10\n")
DPKG_ANSWERS = {
    ("dpkg-query", "-S", "--", GLIB, GLIB[len("/usr") :]): DPKG_S,
    ("dpkg-query", "-W", "-f", b.Dpkg.FIELDS, "libglib2.0-0t64:amd64"): DPKG_W,
}

# Abridged from the glib2.0 copyright file in Ubuntu 24.04.
GLIB_COPYRIGHT = """Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: glib
Source: https://download.gnome.org/sources/glib/

Files: *
Copyright: 1995-2023 GLib contributors
License: LGPL-2.1+

Files: debian/*
Copyright: 1999-2023 Debian GNOME Maintainers
License: GPL-2+

Files: gio/tests/gdbus-object-manager-example/*
 gio/tests/gengiotypefuncs.py
Copyright: 2008-2022 Red Hat, Inc.
License: AFL-2.0 or LGPL-2.1+

Files: glib/gnulib/*
Copyright: 1997-2018 Free Software Foundation, Inc.
License: LGPL-2+

Files: gio/gnetworkmonitornm.c
Copyright: 2014 Red Hat, Inc.
License: GPL-2+ or AFL-2.1, and Expat

Files: m4macros/*
Copyright: 1996-2001 Free Software Foundation, Inc.
License: GPL-3+ with Bison exception

License: LGPL-2.1+
 On Debian systems, the full text of the GNU Lesser General Public License
 version 2.1 can be found in /usr/share/common-licenses/LGPL-2.1.

License: LGPL-2+
 See /usr/share/common-licenses/LGPL-2.
"""

FREE_FORM = """This package was debianized by someone in 1998.

It was downloaded from https://example.org/

Copyright: 1998 Someone
License: see /usr/share/common-licenses/LGPL-2
"""


class DebianLicences(unittest.TestCase):
    def test_expression_keeps_names_and_reads_debian_operators(self):
        self.assertEqual(b.debian_expression("LGPL-2.1+"), "LGPL-2.1+")
        self.assertEqual(b.debian_expression("AFL-2.0 or LGPL-2.1+"), "AFL-2.0 OR LGPL-2.1+")
        self.assertEqual(b.debian_expression("GPL-3+ with Bison exception"), "GPL-3+ WITH Bison-exception")
        self.assertEqual(
            b.debian_expression("Apache-2.0 with LLVM exception"), "Apache-2.0 WITH LLVM-exception"
        )
        self.assertEqual(
            b.debian_expression("GPL-2+ or AFL-2.1, and Expat and Tcl-BSDish"),
            "(GPL-2+ OR AFL-2.1) AND (Expat AND Tcl-BSDish)",
        )

    def test_copyright_licence_joins_every_files_paragraph_but_the_packaging(self):
        expr = b.copyright_licence(GLIB_COPYRIGHT)
        self.assertEqual(
            expr,
            "((GPL-2+ OR AFL-2.1) AND (Expat)) AND (AFL-2.0 OR LGPL-2.1+) AND GPL-3+ WITH Bison-exception"
            " AND LGPL-2+ AND LGPL-2.1+",
        )
        self.assertNotIn("debian", expr)

    def test_free_form_copyright_has_no_licence(self):
        self.assertIsNone(b.copyright_licence(FREE_FORM))
        self.assertIsNone(b.copyright_licence(""))

    def test_licence_names_in_free_text_do_not_parse(self):
        text = GLIB_COPYRIGHT.replace("License: LGPL-2+\n", "License: MIT/X11 (BSD like)\n", 1)
        self.assertIsNone(b.copyright_licence(text))

    def test_every_reviewed_debian_licence_passes_the_policy(self):
        for package, expr in b.DEBIAN_LICENCES.items():
            libs = [lib for (name, lib) in b.ALLOW_GPL if name == package] or ["libany.so.1"]
            self.assertIsNone(b.package_ok(package, expr, libs), package)

    def test_an_empty_licence_field_is_not_machine_readable(self):
        self.assertIsNone(b.copyright_licence(GLIB_COPYRIGHT.replace("License: LGPL-2+\n", "License: ,\n", 1)))

    def test_policy_reads_debian_strings(self):
        self.assertIsNone(b.package_ok("p", "LGPL-2.1+ AND LGPL-2+ AND GPL-3+ WITH Bison-exception", ["libp.so.1"]))
        self.assertIsNone(b.package_ok("p", "GPL-2+ OR AFL-2.1", ["libp.so.1"]))
        self.assertIn("not on the reviewed allowlist", b.package_ok("p", "LGPL-2.1+ AND GPL-2+", ["libp.so.1"]))
        self.assertIn("cannot be parsed", b.package_ok("p", "GPL-2.0+-ish", ["libp.so.1"]))

    def test_every_legacy_entry_maps_to_spdx(self):
        for debian, spdx in b.LEGACY.items():
            self.assertTrue(b.SPDX_GPL.match(spdx) or "GPL" not in spdx, (debian, spdx))

    def test_a_name_with_gpl_anywhere_must_be_spdx_or_mapped(self):
        self.assertIn("cannot be parsed", b.package_ok("p", "Expat AND GNU-GPL-2+", ["libp.so.1"]))
        self.assertIsNone(b.package_ok("p", "Apache-2.0-with-GPL2-LGPL2-Exception AND Zlib", ["libp.so.1"]))


class DpkgDatabase(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, self.tmp, True)
        self.doc = os.path.join(self.tmp, "doc")
        self.common = os.path.join(self.tmp, "common-licenses")
        os.makedirs(os.path.join(self.doc, "libglib2.0-0t64"))
        os.makedirs(self.common)
        text = GLIB_COPYRIGHT.replace("/usr/share/common-licenses", self.common)
        with open(os.path.join(self.doc, "libglib2.0-0t64", "copyright"), "w") as handle:
            handle.write(text)
        for name in ("LGPL-2.1", "LGPL-2"):
            with open(os.path.join(self.common, name), "w") as handle:
                handle.write(f"{name} TEXT")
        self.db = b.Dpkg(doc_root=self.doc, licences_root=self.common)

    def test_owner_names_the_package_its_version_licence_and_source(self):
        with mock.patch.object(b, "run", Recorded(DPKG_ANSWERS)):
            owner = self.db.owners(GLIB)
        self.assertEqual(owner["name"], "libglib2.0-0t64")
        self.assertEqual(owner["version"], "2.80.0-6ubuntu3.10")
        self.assertEqual(owner["source"], "glib2.0_2.80.0-6ubuntu3.10")
        self.assertIn("LGPL-2.1+", owner["license"])

    def test_licence_texts_are_the_copyright_file_and_the_common_licences_it_names(self):
        with mock.patch.object(b, "run", Recorded(DPKG_ANSWERS)):
            owner = self.db.owners(GLIB)
        names = [os.path.basename(p) for p in self.db.licence_texts(owner)]
        self.assertEqual(sorted(names), ["LGPL-2", "LGPL-2.1", "copyright"])

    def test_a_free_form_copyright_takes_the_reviewed_licence_and_says_so(self):
        os.makedirs(os.path.join(self.doc, "libcairo2"))
        with open(os.path.join(self.doc, "libcairo2", "copyright"), "w") as handle:
            handle.write(FREE_FORM)
        cairo = "/usr/lib/x86_64-linux-gnu/libcairo.so.2.11800.0"
        answers = {
            ("dpkg-query", "-S", "--", cairo, cairo[len("/usr") :]): proc(f"libcairo2:amd64: {cairo}\n", 1),
            ("dpkg-query", "-W", "-f", b.Dpkg.FIELDS, "libcairo2:amd64"): proc(
                "libcairo2\t1.18.0-3build1\tcairo\t1.18.0-3build1\n"
            ),
        }
        with mock.patch.object(b, "run", Recorded(answers)):
            owner = self.db.owners(cairo)
        self.assertEqual(owner["license"], b.DEBIAN_LICENCES["libcairo2"])
        self.assertTrue(owner["reviewed_licence"])
        self.assertEqual(owner["source"], "cairo_1.18.0-3build1")
        doc = b.render({"libcairo2": {**owner, "files": ["libcairo.so.2"]}}, {"libcairo2": [("copyright", FREE_FORM)]})
        self.assertIn("  licence: reviewed by hand", doc)

    def test_an_unlisted_free_form_copyright_has_no_licence(self):
        with open(os.path.join(self.doc, "libglib2.0-0t64", "copyright"), "w") as handle:
            handle.write(FREE_FORM)
        with mock.patch.object(b, "run", Recorded(DPKG_ANSWERS)):
            owner = self.db.owners(GLIB)
        self.assertIsNone(owner["license"])
        self.assertFalse(owner["reviewed_licence"])

    def test_diversions_are_not_owners(self):
        lines = (
            f"diversion by dash from: {GLIB}\nlocal diversion from: {GLIB}\n"
            f"libglib2.0-0t64:amd64: {GLIB}\n"
        )
        answers = dict(DPKG_ANSWERS)
        answers[("dpkg-query", "-S", "--", GLIB, GLIB[len("/usr") :])] = proc(lines)
        with mock.patch.object(b, "run", Recorded(answers)):
            self.assertEqual(self.db.owners(GLIB)["name"], "libglib2.0-0t64")

    def test_a_path_dpkg_does_not_know_has_no_owner(self):
        with mock.patch.object(b, "run", Recorded({})):
            self.assertIsNone(self.db.owners("/usr/lib/x86_64-linux-gnu/libnot.so.1"))

    def test_merged_usr_spellings(self):
        self.assertEqual(b.usr_aliases("/usr/lib/x/libz.so.1"), ["/usr/lib/x/libz.so.1", "/lib/x/libz.so.1"])
        self.assertEqual(b.usr_aliases("/lib/x/libz.so.1"), ["/lib/x/libz.so.1", "/usr/lib/x/libz.so.1"])

    def test_describe_and_documents_on_a_debian_host(self):
        libdir = os.path.join(self.tmp, "AppDir", "usr", "lib")
        os.makedirs(libdir)
        with open(os.path.join(libdir, "libglib-2.0.so.0"), "wb") as handle:
            handle.write(b"elf")
        host = mock.Mock(candidates=lambda rel: [GLIB])
        lines = []
        with mock.patch.object(b, "run", Recorded(DPKG_ANSWERS)):
            packages, unknown = b.describe(libdir, [self.db], host, lines.append)
        self.assertEqual(unknown, [])
        self.assertEqual(packages["libglib2.0-0t64"]["files"], ["libglib-2.0.so.0"])
        self.assertIn("  libglib-2.0.so.0: libglib2.0-0t64 (dpkg)", lines)
        sources = b.render_sources(packages)
        self.assertIn("glib2.0_2.80.0-6ubuntu3.10\tlibglib2.0-0t64\t", sources)


class RpmDatabase(unittest.TestCase):
    QF = "%{NAME}\t%{VERSION}-%{RELEASE}\t%{LICENSE}\t%{SOURCERPM}\n"

    def test_owner_from_the_rpm_database(self):
        answers = {
            ("rpm", "-qf", "--qf", self.QF, "/usr/lib64/libffi.so.8.1.4"): proc(
                "libffi\t3.4.6-5.fc44\tMIT AND GPL-2.0-or-later WITH Texinfo-exception\tlibffi-3.4.6-5.fc44.src.rpm\n"
            )
        }
        with mock.patch.object(b, "run", Recorded(answers)):
            owner = b.Rpm().owners("/usr/lib64/libffi.so.8.1.4")
            self.assertIsNone(b.Rpm().owners("/usr/lib64/libnot.so.1"))
        self.assertEqual(owner["name"], "libffi")
        self.assertEqual(owner["version"], "3.4.6-5.fc44")
        self.assertEqual(owner["source"], "libffi-3.4.6-5.fc44")
        self.assertEqual(owner["source_name"], "libffi")
        self.assertIsNone(b.package_ok("libffi", owner["license"], ["libffi.so.8"]))


class NoDatabase(unittest.TestCase):
    INDEX = proc("\tlibc.so.6 (libc6,x86-64) => /lib/x86_64-linux-gnu/libc.so.6\n")

    def test_neither_database_knowing_the_c_library_fails_at_once(self):
        fake = Recorded({("ldconfig", "-p"): self.INDEX})
        with mock.patch.object(b, "run", fake), mock.patch.object(b.shutil, "which", return_value="/usr/bin/x"):
            with self.assertRaises(notices.LicenceError) as caught:
                b.package_databases()
        self.assertIn("neither the rpm nor the dpkg database", str(caught.exception))
        self.assertEqual({c[0] for c in fake.calls}, {"ldconfig", "rpm", "dpkg-query"})

    def test_a_library_no_database_owns_fails_naming_it(self):
        tmp = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, tmp, True)
        with open(os.path.join(tmp, "libmystery.so.1"), "wb") as handle:
            handle.write(b"elf")
        lines = []
        host = mock.Mock(candidates=lambda rel: ["/usr/lib/x86_64-linux-gnu/libmystery.so.1"])
        with mock.patch.object(b, "run", Recorded({})):
            packages, unknown = b.describe(tmp, [b.Rpm(), b.Dpkg()], host, lines.append)
        self.assertEqual((packages, unknown), ({}, ["libmystery.so.1"]))
        self.assertIn("  libmystery.so.1: no system package owns it", lines)

    def test_a_query_that_does_not_answer_fails_instead_of_hanging(self):
        timeout = subprocess.TimeoutExpired(["rpm", "-qa"], b.QUERY_TIMEOUT)
        with mock.patch.object(b.subprocess, "run", side_effect=timeout):
            with self.assertRaises(notices.LicenceError) as caught:
                b.run(["rpm", "-qa"])
        self.assertIn("gave no answer", str(caught.exception))


class HostLookup(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, self.tmp, True)
        self.lib = os.path.join(self.tmp, "lib", "x86_64-linux-gnu")
        os.makedirs(os.path.join(self.lib, "gio", "modules"))
        for rel in ("libz.so.1", "gio/modules/libgiognutls.so"):
            with open(os.path.join(self.lib, rel), "wb") as handle:
                handle.write(b"elf")
        # A folder that links back to its parent: a walk that followed it would never end.
        os.symlink(self.lib, os.path.join(self.lib, "gio", "loop"))

    def host(self):
        host = b.HostFiles({"libz.so.1": [os.path.join(self.lib, "libz.so.1")]})
        host.dirs = [self.lib]
        return host

    def test_modules_are_found_by_relative_path_without_a_walk(self):
        host = self.host()
        found = host.candidates("gio/modules/libgiognutls.so")
        self.assertEqual(found, [os.path.join(self.lib, "gio", "modules", "libgiognutls.so")])
        self.assertIsNone(host.by_name)

    def test_the_walk_runs_once_and_does_not_follow_links(self):
        host = self.host()
        self.assertEqual(host.candidates("other/libgiognutls.so"), [os.path.join(self.lib, "gio", "modules", "libgiognutls.so")])
        walked = host.by_name
        self.assertEqual(host.candidates("libnone.so"), [])
        self.assertIs(host.by_name, walked)
        self.assertEqual(walked["libgiognutls.so"], [os.path.join(self.lib, "gio", "modules", "libgiognutls.so")])


if __name__ == "__main__":
    unittest.main()
