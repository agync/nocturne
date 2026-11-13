"""Tests for scripts/notices.py. Run: python3 -I -m unittest discover -s scripts -p 'test_*.py'"""

import json
import os
import shutil
import subprocess
import tempfile
import unittest

import notices

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)


def crate(name, licence, dir_="/nonexistent"):
    return {
        "kind": "crate",
        "name": name,
        "version": "1.0.0",
        "license": licence,
        "dir": dir_,
        "license_file": None,
        "holder": "Someone",
        "source": "https://example.invalid/" + name,
    }


def metadata(packages, edges):
    """A cargo metadata document: `packages` are (name, licence, source), edges name -> deps."""
    ids = {n: f"{n} 1.0.0 (id)" for n, _, _ in packages}
    pkgs = [
        {
            "id": ids[n],
            "name": n,
            "version": "1.0.0",
            "license": lic,
            "source": src,
            "manifest_path": f"/nonexistent/{n}/Cargo.toml",
            "license_file": None,
        }
        for n, lic, src in packages
    ]
    nodes = []
    for n, _, _ in packages:
        deps = [
            {"pkg": ids[d], "dep_kinds": [{"kind": kind}]}
            for d, kind in edges.get(n, [])
        ]
        nodes.append({"id": ids[n], "deps": deps})
    return {
        "packages": pkgs,
        "workspace_members": [ids["nocturne-app"]],
        "resolve": {"nodes": nodes, "root": None},
    }


class Policy(unittest.TestCase):
    def test_allowed_licences_pass(self):
        pkgs = [crate("a", "MIT"), crate("b", "MIT OR Apache-2.0"), crate("c", "MIT/Apache-2.0")]
        self.assertEqual(notices.check_policy(pkgs), [])

    def test_unknown_licence_fails(self):
        failures = notices.check_policy([crate("odd-crate", "GPL-3.0-only")])
        self.assertEqual(len(failures), 1)
        self.assertIn("odd-crate", failures[0])

    def test_missing_licence_fails(self):
        failures = notices.check_policy([crate("bare", "")])
        self.assertIn("bare", failures[0])

    def test_or_expressions_take_any_allowed(self):
        self.assertTrue(notices.evaluate("GPL-3.0-only OR MIT"))
        self.assertFalse(notices.evaluate("GPL-3.0-only OR LGPL-2.1-only"))

    def test_and_expressions_need_all(self):
        self.assertTrue(notices.evaluate("MIT AND Unicode-3.0"))
        self.assertFalse(notices.evaluate("MIT AND GPL-3.0-only"))
        self.assertTrue(notices.evaluate("(MIT OR Apache-2.0) AND BSD-3-Clause"))

    def test_with_exceptions(self):
        self.assertTrue(notices.evaluate("Apache-2.0 WITH LLVM-exception"))
        self.assertTrue(notices.evaluate("MIT OR Apache-2.0 WITH LLVM-exception"))
        self.assertFalse(notices.evaluate("GPL-2.0-only WITH Classpath-exception-2.0"))

    def test_data_licence_allowed(self):
        self.assertTrue(notices.evaluate("CDLA-Permissive-2.0"))


class Collection(unittest.TestCase):
    def test_only_normal_dependencies_of_the_app_are_listed(self):
        meta = metadata(
            [
                ("nocturne-app", "Apache-2.0", None),
                ("shipped", "MIT", "registry+x"),
                ("build-only", "GPL-3.0-only", "registry+x"),
                ("test-only", "GPL-3.0-only", "registry+x"),
                ("nested", "ISC", "registry+x"),
            ],
            {
                "nocturne-app": [("shipped", None), ("build-only", "build"), ("test-only", "dev")],
                "shipped": [("nested", None)],
            },
        )
        names = [p["name"] for p in notices.rust_packages(meta)]
        self.assertEqual(names, ["nested", "shipped"])

    def test_workspace_crates_are_not_listed(self):
        meta = metadata(
            [("nocturne-app", "Apache-2.0", None), ("nocturne-engine", "Apache-2.0", None)],
            {"nocturne-app": [("nocturne-engine", None)]},
        )
        self.assertEqual(notices.rust_packages(meta), [])


class Output(unittest.TestCase):
    def fonts_and_lemonade(self):
        tmp = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, tmp, True)
        packages = []
        for name, text in (
            ("@fontsource-variable/newsreader", "Newsreader OFL text"),
            ("@fontsource/ibm-plex-sans", "Plex Sans OFL text"),
        ):
            d = os.path.join(tmp, name.replace("/", "_"))
            os.makedirs(d)
            with open(os.path.join(d, "LICENSE"), "w") as f:
                f.write(text)
            pkg = crate(name, "OFL-1.1", d)
            pkg["kind"] = "npm"
            packages.append(pkg)
        return packages

    def test_fonts_and_lemonade_included(self):
        out = notices.build_notices(self.fonts_and_lemonade(), "Lemonade Apache text", "9.9.9")
        for needle in (
            "Newsreader",
            "IBM Plex Sans",
            "Lemonade 9.9.9",
            "Lemonade Apache text",
            "Newsreader OFL text",
            "Plex Sans OFL text",
        ):
            self.assertIn(needle, out)

    def test_identical_texts_are_printed_once(self):
        tmp = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, tmp, True)
        pkgs = []
        for name in ("one", "two"):
            d = os.path.join(tmp, name)
            os.makedirs(d)
            with open(os.path.join(d, "LICENSE-APACHE"), "w") as f:
                f.write("THE SHARED TEXT")
            pkgs.append(crate(name, "Apache-2.0", d))
        out = notices.build_notices(pkgs, "x")
        self.assertEqual(out.count("THE SHARED TEXT"), 1)
        self.assertIn("shared text 1", out)

    def test_package_without_a_licence_file_gets_a_standard_text(self):
        out = notices.build_notices([crate("bare", "MIT")], "x")
        self.assertIn("Copyright held by: Someone", out)
        self.assertIn("Permission is hereby granted", out)


class Offline(unittest.TestCase):
    @unittest.skipUnless(shutil.which("unshare") and shutil.which("cargo"), "needs unshare and cargo")
    def test_offline(self):
        tmp = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, tmp, True)
        with open(os.path.join(tmp, "LICENSE"), "w") as f:
            f.write("Lemonade licence")
        probe = subprocess.run(["unshare", "-rn", "true"], capture_output=True)
        if probe.returncode != 0:
            self.skipTest("no user and network namespaces here")
        env = dict(os.environ, CARGO_NET_OFFLINE="true")
        proc = subprocess.run(
            ["unshare", "-rn", "python3", "-I", os.path.join(HERE, "notices.py"), "--check", "--lemonade", tmp],
            capture_output=True,
            text=True,
            env=env,
            cwd=ROOT,
        )
        self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
        self.assertIn("all licences allowed", proc.stdout)


if __name__ == "__main__":
    unittest.main()
