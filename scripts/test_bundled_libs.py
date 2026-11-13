"""Tests for scripts/bundled_libs.py: the copyleft policy, pruning and the documents."""

import os
import shutil
import tempfile
import unittest

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


class Documents(unittest.TestCase):
    PKGS = {
        "libx": {"name": "libx", "version": "1-1", "license": "MIT", "srpm": "x-1-1.fc44", "files": ["libx.so.1"]},
        "liby": {"name": "liby", "version": "2-1", "license": "LGPL-2.1-or-later", "srpm": "y-2-1.fc44", "files": ["liby.so.2"]},
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


if __name__ == "__main__":
    unittest.main()
