"""Tests for scripts/check_flatpak.py."""

import os
import shutil
import tempfile
import unittest

import check_flatpak

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
FILES = (
    check_flatpak.MANIFEST,
    check_flatpak.METAINFO,
    check_flatpak.DESKTOP,
    check_flatpak.AETHERPAK,
    check_flatpak.DOC,
    "lemonade.lock",
    "Cargo.toml",
)


class Repository(unittest.TestCase):
    def test_the_flatpak_files_agree(self):
        self.assertEqual(check_flatpak.problems(ROOT), [])

    def test_every_permission_is_found(self):
        manifest = check_flatpak.read(ROOT, check_flatpak.MANIFEST)
        self.assertIn("--device=all", check_flatpak.finish_args(manifest))
        self.assertNotIn("--filesystem=home", check_flatpak.finish_args(manifest))


class Drift(unittest.TestCase):
    def setUp(self):
        self.root = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, self.root)
        for path in FILES:
            os.makedirs(os.path.join(self.root, os.path.dirname(path)), exist_ok=True)
            shutil.copy(os.path.join(ROOT, path), os.path.join(self.root, path))

    def edit(self, path, old, new):
        full = os.path.join(self.root, path)
        with open(full, encoding="utf-8") as handle:
            text = handle.read()
        self.assertIn(old, text)
        with open(full, "w", encoding="utf-8") as handle:
            handle.write(text.replace(old, new, 1))

    def problems(self):
        return "\n".join(check_flatpak.problems(self.root))

    def test_a_new_engine_pin(self):
        self.edit("lemonade.lock", 'sha256 = "6', 'sha256 = "7')
        self.assertIn("x86_64 engine archive", self.problems())

    def test_a_new_version(self):
        self.edit("Cargo.toml", 'version = "', 'version = "9')
        self.assertIn("newest release", self.problems())

    def test_an_unexplained_permission(self):
        self.edit(check_flatpak.MANIFEST, "  - --share=ipc\n", "  - --share=ipc\n  - --filesystem=home\n")
        self.assertIn("--filesystem=home is in the manifest but not explained", self.problems())

    def test_a_permission_dropped_from_the_manifest(self):
        self.edit(check_flatpak.MANIFEST, "  - --socket=pulseaudio\n", "")
        self.assertIn("--socket=pulseaudio is explained but not in the manifest", self.problems())

    def test_another_app_id(self):
        self.edit(check_flatpak.AETHERPAK, "app_id: is.nocturne.Nocturne", "app_id: is.nocturne.Other")
        self.assertIn("the app id is is.nocturne.Other", self.problems())


if __name__ == "__main__":
    unittest.main()
