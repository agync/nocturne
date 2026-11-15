"""Tests for scripts/ci-areas.sh: which CI jobs a list of changed paths selects."""

import os
import re
import subprocess
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
SCRIPT = os.path.join(HERE, "ci-areas.sh")
WORKFLOW = os.path.join(ROOT, ".github", "workflows", "ci.yml")

RUST = {"rust", "rust-platforms", "scripts"}
PACKAGE = {"package", "package-linux"}

# One changed path for each alternative in the table, and the areas it alone selects.
CASES = (
    (".cargo/config.toml", RUST),
    ("clippy.toml", RUST),
    ("Cargo.toml", RUST | PACKAGE),
    ("Cargo.lock", RUST | PACKAGE),
    ("helpers.lock", RUST | PACKAGE),
    ("lemonade.lock", RUST | PACKAGE | {"site"}),
    ("crates/engine/src/lib.rs", RUST),
    ("crates/engine/tests/round2.rs", RUST),
    ("crates/helpers/fixtures/help/tailnet/1.104.6/help.txt", RUST),
    ("src-tauri/src/remote/mod.rs", RUST | {"ui"}),
    ("src-tauri/tests/ipc.rs", RUST | {"ui"}),
    ("src-tauri/bundle/macos.json", RUST | PACKAGE | {"ui"}),
    ("src-tauri/icons/icon.icns", RUST | PACKAGE | {"ui"}),
    ("src-tauri/capabilities/default.json", RUST | {"ui"}),
    ("src-tauri/build.rs", RUST | PACKAGE | {"ui"}),
    ("ui/src/app/Shell.tsx", {"rust", "ui", "ui-platforms"}),
    ("ui/package.json", {"rust", "ui", "ui-platforms", "scripts", "site"} | PACKAGE),
    ("ui/package-lock.json", {"rust", "ui", "ui-platforms", "scripts", "site"} | PACKAGE),
    ("scripts/bindings.sh", {"rust", "scripts"} | PACKAGE),
    ("scripts/away-integration.sh", {"scripts"} | PACKAGE),
    ("scripts/fetch-lemonade.sh", {"rust-platforms", "scripts"} | PACKAGE),
    ("scripts/check_ui_dist.py", {"scripts", "ui"} | PACKAGE),
    ("scripts/check_tree.py", {"scripts", "ui"} | PACKAGE),
    ("scripts/build-tokens.mjs", {"ui", "ui-platforms", "scripts"} | PACKAGE),
    ("scripts/package.sh", {"scripts"} | PACKAGE),
    ("scripts/test-tailnet.sh", {"scripts", "tailnet"} | PACKAGE),
    ("scripts/tailnet-tags.sh", {"scripts", "tailnet"} | PACKAGE),
    ("scripts/build-tailnet.sh", {"scripts", "tailnet"} | PACKAGE),
    ("flatpak/is.nocturne.Nocturne.yml", {"scripts", "package-linux"}),
    ("docs/contribution/flatpak.md", {"scripts", "site"}),
    ("docs/guide/install.md", {"site"}),
    ("docs/ui/notes.md", {"site"}),
    ("site/index.html", {"site"}),
    ("LICENSE", {"scripts"} | PACKAGE),
    ("helpers/tailnet/main.go", {"tailnet"}),
    ("README.md", set()),
    (".github/dependabot.yml", set()),
)


def areas(*args, paths=()):
    proc = subprocess.run(
        [SCRIPT, *args],
        input="".join(p + "\n" for p in paths),
        capture_output=True,
        text=True,
        check=True,
    )
    result = dict(line.split("=", 1) for line in proc.stdout.splitlines())
    return {name for name, run in result.items() if run == "true"}, list(result)


def selected(*paths):
    return areas(paths=paths)[0]


EVERY = set(areas("--all")[1])


class Table(unittest.TestCase):
    def test_every_area_the_workflow_reads_is_written(self):
        with open(WORKFLOW, encoding="utf-8") as f:
            read = set(re.findall(r"needs\.changes\.outputs\.([A-Za-z0-9_-]+)", f.read()))
        self.assertEqual(read, EVERY)

    def test_all_and_an_empty_change(self):
        self.assertEqual(areas("--all")[0], EVERY)
        self.assertEqual(selected(), set())

    def test_a_change_to_ci_runs_everything(self):
        for path in (
            ".github/workflows/release.yml",
            "Makefile",
            "rust-toolchain.toml",
            "scripts/ci-areas.sh",
        ):
            with self.subTest(path=path):
                self.assertEqual(selected(path), EVERY)

    def test_each_path_selects_its_areas(self):
        for path, expected in CASES:
            with self.subTest(path=path):
                self.assertEqual(selected(path), expected)

    def test_a_change_selects_the_union_of_its_paths(self):
        self.assertEqual(
            selected("README.md", "docs/guide/install.md", "site/index.html"), {"site"}
        )
        self.assertEqual(
            selected("src-tauri/tests/ipc.rs", "scripts/package.sh"),
            RUST | PACKAGE | {"ui"},
        )


if __name__ == "__main__":
    unittest.main()
