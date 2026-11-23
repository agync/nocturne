"""Tests for scripts/affected_crates.py: which workspace packages a change needs tested."""

import glob
import io
import json
import os
import re
import subprocess
import sys
import tempfile
import tomllib
import unittest
from contextlib import redirect_stderr, redirect_stdout

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)
import affected_crates as ac  # noqa: E402

# The workspace's shape: each package's folder and the workspace packages it depends on.
SHAPE = {
    "nocturne-contract": ("crates/contract", []),
    "nocturne-channel": ("crates/channel", ["nocturne-contract"]),
    "nocturne-engine": ("crates/engine", ["nocturne-contract"]),
    "nocturne-intelligence": ("crates/intelligence", ["nocturne-contract", "nocturne-engine"]),
    "nocturne-lemonade": ("crates/lemonade", ["nocturne-contract", "nocturne-engine"]),
    "nocturne-helpers": ("crates/helpers", ["nocturne-engine", "nocturne-lemonade"]),
    "nocturne-app": (
        "src-tauri",
        [
            "nocturne-channel",
            "nocturne-contract",
            "nocturne-engine",
            "nocturne-helpers",
            "nocturne-intelligence",
            "nocturne-lemonade",
        ],
    ),
    "nocturne-phone": (
        "phone/src-tauri",
        [
            "nocturne-channel",
            "nocturne-contract",
            "nocturne-engine",
            "tauri-plugin-nocturne-calendar",
            "tauri-plugin-nocturne-speech",
        ],
    ),
    "tauri-plugin-nocturne-calendar": ("phone/plugins/calendar", ["nocturne-contract"]),
    "tauri-plugin-nocturne-speech": ("phone/plugins/speech", []),
}


def metadata(shape=SHAPE, root="/ws"):
    """The parts of `cargo metadata --no-deps` the script reads, with an outside crate as noise."""
    packages = []
    for name, (folder, deps) in shape.items():
        dependencies = [{"name": d, "kind": None} for d in deps]
        dependencies.append({"name": "serde", "kind": None})
        dependencies.append({"name": name, "kind": "dev"})
        packages.append({"name": name, "manifest_path": f"{root}/{folder}/Cargo.toml", "dependencies": dependencies})
    return {"workspace_root": root, "packages": packages}


def selected(*paths, meta=None):
    found, _ = ac.affected(list(paths), meta or metadata())
    return found


ALL = None
APP_SIDE = {"nocturne-app", "nocturne-phone"}


class Selection(unittest.TestCase):
    def test_a_leaf_crate_selects_itself_alone(self):
        self.assertEqual(selected("src-tauri/tests/ipc.rs"), {"nocturne-app"})
        self.assertEqual(selected("phone/src-tauri/src/lib.rs"), {"nocturne-phone"})

    def test_a_crate_source_selects_the_lemonade_test_that_reads_it(self):
        self.assertEqual(
            selected("src-tauri/src/remote/mod.rs"), {"nocturne-app", "nocturne-lemonade"}
        )

    def test_a_plugin_selects_itself_and_the_phone(self):
        self.assertEqual(
            selected("phone/plugins/speech/src/lib.rs"),
            {"tauri-plugin-nocturne-speech", "nocturne-phone"},
        )

    def test_the_engine_selects_every_package_that_depends_on_it(self):
        self.assertEqual(
            selected("crates/engine/src/journal.rs"),
            {
                "nocturne-engine",
                "nocturne-intelligence",
                "nocturne-lemonade",
                "nocturne-helpers",
                "nocturne-app",
                "nocturne-phone",
            },
        )

    def test_dependents_are_followed_through_more_than_one_step(self):
        # helpers depends on lemonade, and the app on helpers.
        self.assertEqual(
            selected("crates/helpers/tests/tailnet.rs"), {"nocturne-helpers", "nocturne-app"}
        )

    def test_dev_and_build_dependencies_count(self):
        shape = dict(SHAPE)
        shape["nocturne-x"] = ("crates/x", [])
        meta = metadata(shape)
        for p in meta["packages"]:
            if p["name"] == "nocturne-channel":
                p["dependencies"].append({"name": "nocturne-x", "kind": "dev"})
            if p["name"] == "tauri-plugin-nocturne-speech":
                p["dependencies"].append({"name": "nocturne-x", "kind": "build"})
        self.assertEqual(
            selected("crates/x/tests/x.rs", meta=meta),
            {"nocturne-x", "nocturne-channel", "tauri-plugin-nocturne-speech"} | APP_SIDE,
        )

    def test_a_crate_manifest_selects_its_crate(self):
        self.assertEqual(selected("src-tauri/Cargo.toml"), {"nocturne-app"})

    def test_what_every_build_reads_selects_the_workspace(self):
        for path in (
            "Cargo.lock",
            "Cargo.toml",
            "rust-toolchain.toml",
            ".cargo/config.toml",
            ".config/nextest.toml",
            "clippy.toml",
            "Makefile",
            ".github/workflows/ci.yml",
            ".github/actions/rust-toolchain/action.yml",
            "scripts/affected_crates.py",
            "scripts/ci-areas.sh",
        ):
            with self.subTest(path=path):
                self.assertIs(selected(path), ALL)
                self.assertIs(selected("docs/overview.md", path), ALL)

    def test_a_rust_tree_file_in_no_package_selects_the_workspace(self):
        self.assertIs(selected("phone/package.json"), ALL)
        self.assertIs(selected("crates/README.md"), ALL)

    def test_paths_away_from_rust_select_nothing(self):
        for path in (
            "docs/design/overview.md",
            "docs/logs/2026-11-23.md",
            "README.md",
            "site/index.html",
            "helpers/tailnet/main.go",
            "scripts/package.sh",
            "flatpak/is.nocturne.Nocturne.yml",
            "lemonade.lock",
        ):
            with self.subTest(path=path):
                self.assertEqual(selected(path), set())

    def test_a_docs_only_change_selects_nothing(self):
        self.assertEqual(selected("docs/overview.md", "docs/design/index.md"), set())

    def test_files_read_from_outside_a_package(self):
        cases = (
            ("ui/src/app/Shell.tsx", {"nocturne-app"}),
            ("docs/guide/install.md", {"nocturne-app"}),
            ("ui/src/copy/en-GB/phone-health.json", {"nocturne-app", "tauri-plugin-nocturne-calendar", "nocturne-phone"}),
            ("ui/src/engine/bindings/RemoteCall.ts", {"nocturne-app", "nocturne-contract"}),
            ("helpers.lock", {"nocturne-helpers", "nocturne-app"}),
            ("CHANGELOG.md", {"nocturne-channel"}),
            ("src-tauri/tests/fixtures/remote-vectors.json", {"nocturne-app", "nocturne-channel"}),
            ("phone/src-tauri/src/sync/mod.rs", {"nocturne-phone", "nocturne-app"}),
        )
        for path, expected in cases:
            with self.subTest(path=path):
                self.assertEqual(selected(path), expected)

    def test_a_test_read_selects_the_reader_alone(self):
        # The lemonade test reads the channel's source; lemonade's dependents are not added.
        self.assertEqual(
            selected("crates/channel/src/protocol.rs"),
            {"nocturne-channel", "nocturne-lemonade"} | APP_SIDE,
        )

    def test_a_change_selects_the_union_of_its_paths(self):
        self.assertEqual(
            selected("phone/plugins/speech/src/lib.rs", "src-tauri/tests/ipc.rs", "README.md"),
            {"tauri-plugin-nocturne-speech", "nocturne-phone", "nocturne-app"},
        )

    def test_windows_paths_in_the_metadata(self):
        meta = metadata(root="C:\\ws")
        for p in meta["packages"]:
            p["manifest_path"] = p["manifest_path"].replace("/", "\\")
        self.assertEqual(selected("src-tauri/tests/ipc.rs", meta=meta), {"nocturne-app"})


class Output(unittest.TestCase):
    def run_main(self, args, stdin=""):
        out, err = io.StringIO(), io.StringIO()
        old = sys.stdin
        sys.stdin = io.StringIO(stdin)
        try:
            with redirect_stdout(out), redirect_stderr(err):
                code = ac.main(args)
        finally:
            sys.stdin = old
        return code, out.getvalue(), err.getvalue()

    def with_metadata(self, stdin):
        with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as f:
            json.dump(metadata(), f)
        try:
            return self.run_main(["--metadata", f.name], stdin)
        finally:
            os.unlink(f.name)

    def test_all(self):
        self.assertEqual(
            self.run_main(["--all"])[:2], (0, "scope=all\npackages=--workspace\napp=true\n")
        )

    def test_some(self):
        code, out, err = self.with_metadata("phone/plugins/speech/src/lib.rs\n")
        self.assertEqual(code, 0)
        self.assertEqual(
            out,
            "scope=some\npackages=-p nocturne-phone -p tauri-plugin-nocturne-speech\napp=false\n",
        )
        self.assertIn("tauri-plugin-nocturne-speech: phone/plugins/speech/src/lib.rs", err)

    def test_the_app_is_flagged(self):
        out = self.with_metadata("src-tauri/tests/ipc.rs\n")[1]
        self.assertEqual(out, "scope=some\npackages=-p nocturne-app\napp=true\n")

    def test_none(self):
        out = self.with_metadata("docs/overview.md\n\n")[1]
        self.assertEqual(out, "scope=none\npackages=\napp=false\n")

    def test_the_whole_workspace_from_a_path(self):
        code, out, err = self.with_metadata("Cargo.lock\n")
        self.assertEqual(out, "scope=all\npackages=--workspace\napp=true\n")
        self.assertIn("Cargo.lock", err)

    def test_an_unknown_argument_is_refused(self):
        for args in (["--al"], ["--all", "x"], ["--metadata"], ["x"]):
            with self.subTest(args=args):
                code, out, _ = self.run_main(args)
                self.assertEqual((code, out), (2, ""))


def tracked():
    out = subprocess.run(
        ["git", "-c", "core.quotePath=false", "ls-files"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    return set(out.splitlines())


def packages_from_manifests():
    """Each workspace package's name and folder, read from the manifests without cargo."""
    with open(os.path.join(ROOT, "Cargo.toml"), "rb") as f:
        members = tomllib.load(f)["workspace"]["members"]
    found = {}
    for member in members:
        for folder in sorted(glob.glob(os.path.join(ROOT, member))):
            manifest = os.path.join(folder, "Cargo.toml")
            if os.path.isfile(manifest):
                with open(manifest, "rb") as f:
                    name = tomllib.load(f)["package"]["name"]
                found[name] = os.path.relpath(folder, ROOT).replace(os.sep, "/")
    return found


LITERAL = re.compile(r'"((?:[^"\\\n]|\\.)*)"')


def escapes(text, source_file, folder, files):
    """The tracked paths outside `folder` that a string literal of `source_file` leads to."""
    here = os.path.dirname(source_file)
    found = set()
    for literal in LITERAL.findall(text):
        if "../" not in literal:
            continue
        # A format string's leading placeholder is the manifest folder; stop at the next one.
        literal = re.sub(r"^\{\}", "", literal).split("{", 1)[0]
        bases = [folder] if literal.startswith("/") else [here, folder]
        for base in bases:
            target = os.path.normpath(os.path.join(base, literal.lstrip("/"))).replace(os.sep, "/")
            if target == ".":
                target = ""
            if target.startswith("..") or target == folder or target.startswith(folder + "/"):
                continue
            prefix = target + "/" if target else ""
            if target in files or any(f.startswith(prefix) for f in files):
                found.add(target)
    return found


class Tables(unittest.TestCase):
    """The read tables against the tree: a new read from outside a package must be listed."""

    @classmethod
    def setUpClass(cls):
        cls.files = tracked()
        cls.packages = packages_from_manifests()

    def entries(self, name):
        return [re.compile(p) for p, n in ac.BUILT_FROM + ac.TESTS_READ if n == name]

    def test_the_tables_name_workspace_packages(self):
        for _, name in ac.BUILT_FROM + ac.TESTS_READ:
            self.assertIn(name, self.packages)

    def test_each_entry_matches_a_tracked_file(self):
        for pattern, name in ac.BUILT_FROM + ac.TESTS_READ:
            with self.subTest(pattern=pattern, package=name):
                self.assertTrue(any(re.match(pattern, f) for f in self.files))

    def test_every_read_from_outside_a_package_is_listed(self):
        missing = []
        for name, folder in self.packages.items():
            entries = self.entries(name)
            for source in sorted(f for f in self.files if f.startswith(folder + "/") and f.endswith(".rs")):
                with open(os.path.join(ROOT, source), encoding="utf-8") as f:
                    text = f.read()
                for target in escapes(text, source, folder, self.files):
                    prefix = target + "/" if target else ""
                    under = [f for f in self.files if f == target or f.startswith(prefix)]
                    if not any(e.match(f) for e in entries for f in under):
                        missing.append(f"{name}: {source} reads {target or '.'}")
        self.assertEqual(missing, [], "add these to BUILT_FROM or TESTS_READ in affected_crates.py")

    def test_the_scanner_sees_the_kinds_of_read_the_tree_uses(self):
        files = {"ui/src/a.ts", "docs/guide/x.md", "phone/src-tauri/src/sync/mod.rs", "src-tauri/x"}
        text = (
            'include_str!("../../ui/src/a.ts");\n'
            'concat!(env!("CARGO_MANIFEST_DIR"), "/../docs/guide/x.md");\n'
            'format!("{}/../ui/{rel}", env!("CARGO_MANIFEST_DIR"));\n'
            '#[path = "../../phone/src-tauri/src/sync/mod.rs"]\n'
            'let inside = "../x";\n'
            'let nowhere = "../../escape";\n'
        )
        self.assertEqual(
            escapes(text, "src-tauri/tests/probe.rs", "src-tauri", files),
            {"ui/src/a.ts", "docs/guide/x.md", "ui", "phone/src-tauri/src/sync/mod.rs"},
        )


if __name__ == "__main__":
    unittest.main()
