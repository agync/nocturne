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
# A Rust change the other platforms' jobs skip on a pull request.
RUST_HOST = {"rust", "scripts"}
PACKAGE = {"package", "package-linux"}

# One changed path for each alternative in the table, and the areas it alone selects.
CASES = (
    ("clippy.toml", RUST),
    ("Cargo.toml", RUST | PACKAGE | {"android", "phone-engine"}),
    ("Cargo.lock", RUST | PACKAGE | {"android", "phone-engine"}),
    ("helpers.lock", RUST | PACKAGE),
    ("lemonade.lock", RUST | PACKAGE | {"site"}),
    ("crates/engine/src/lib.rs", RUST_HOST | {"phone-engine"}),
    ("crates/engine/tests/round2.rs", RUST_HOST | {"phone-engine"}),
    ("crates/engine/src/private.rs", RUST_HOST | {"phone-engine"}),
    ("crates/mind/src/prompts.rs", RUST_HOST | {"phone-engine"}),
    ("crates/helpers/fixtures/help/tailnet/1.104.6/help.txt", RUST),
    ("crates/channel/src/words.txt", RUST | {"ui"} | {"android"}),
    ("crates/contract/src/lib.rs", RUST_HOST | {"phone-engine"}),
    ("crates/intelligence/src/journal.rs", RUST_HOST),
    ("crates/intelligence/src/host/macos.rs", RUST),
    ("crates/lemonade/src/process.rs", RUST),
    ("crates/helpers/src/supervise.rs", RUST),
    ("crates/channel/src/protocol.rs", RUST | {"ui", "android"}),
    ("crates/engine/build.rs", RUST | {"phone-engine"}),
    (".config/nextest.toml", RUST),
    (".cargo/config.toml", RUST | {"phone-engine"}),
    ("src-tauri/src/remote/mod.rs", RUST | {"ui"}),
    ("src-tauri/tests/ipc.rs", RUST | {"ui"}),
    ("src-tauri/bundle/macos.json", RUST | PACKAGE | {"ui"}),
    ("src-tauri/icons/icon.icns", RUST | PACKAGE | {"ui"}),
    ("src-tauri/capabilities/default.json", RUST | {"ui"}),
    ("src-tauri/build.rs", RUST | PACKAGE | {"ui"}),
    ("ui/src/app/Shell.tsx", {"rust", "ui", "ui-platforms"}),
    ("ui/package.json", {"rust", "ui", "ui-platforms", "scripts", "site", "android"} | PACKAGE),
    (
        "ui/package-lock.json",
        {"rust", "ui", "ui-platforms", "scripts", "site", "android"} | PACKAGE,
    ),
    ("ui/src/phone/App.tsx", {"rust", "ui", "ui-platforms", "android"}),
    ("ui/phone/index.html", {"rust", "ui", "ui-platforms", "android"}),
    ("ui/vite.phone.config.ts", {"rust", "ui", "ui-platforms", "android"}),
    ("phone/src-tauri/src/lib.rs", RUST | {"android"}),
    ("phone/plugins/system/android/build.gradle.kts", RUST | {"android"}),
    ("scripts/bindings.sh", {"rust", "scripts"} | PACKAGE),
    ("scripts/away-integration.sh", {"rust", "scripts"} | PACKAGE),
    ("scripts/away-test-programs.sh", {"rust", "scripts"} | PACKAGE),
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
    # The app bundles the guide, and the Rust tests read it.
    ("docs/guide/install.md", {"site"} | RUST_HOST),
    ("docs/ui/notes.md", {"site"}),
    ("site/index.html", {"site"}),
    ("LICENSE", {"scripts"} | PACKAGE),
    ("helpers/tailnet/main.go", {"tailnet"}),
    ("README.md", set()),
    (".github/dependabot.yml", set()),
)


# The flag each job of ci.yml runs on; None for the jobs that always run.
JOBS = {
    "changes": None,
    "hygiene": None,
    "gate": None,
    "rust": "rust",
    "rust-platforms": "rust-platforms",
    "rust-lint-platforms": "rust-platforms",
    "ui": "ui",
    "e2e": "ui",
    "ui-platforms": "ui-platforms",
    "scripts": "scripts",
    "tailnet": "tailnet",
    "site": "site",
    "package-linux": "package-linux",
    "package-macos": "package",
    "package-windows": "package",
    "android": "android",
    "android-engine": "phone-engine",
}


def workflow_jobs():
    """Each job of ci.yml with its `if:` and the jobs it needs, read from the YAML's layout."""
    with open(WORKFLOW, encoding="utf-8") as f:
        lines = f.read().split("\n")
    start = next(i for i, line in enumerate(lines) if re.fullmatch(r"jobs:\s*(#.*)?", line))
    jobs, job, key = {}, None, None
    for line in lines[start + 1 :]:
        if m := re.fullmatch(r"  ([A-Za-z_][A-Za-z0-9_-]*):\s*(#.*)?", line):
            job = jobs[m[1]] = {"if": None, "needs": [], "continue-on-error": False}
        elif job is None:
            assert not line.strip() or line.lstrip().startswith("#"), f"not a job: {line!r}"
        elif re.fullmatch(r"  [^\s#].*", line):
            raise AssertionError(f"a job id the parser does not read: {line!r}")
        elif m := re.fullmatch(r"    (if|needs|continue-on-error):(.*)", line):
            key, value = m[1], m[2].strip()
            if key == "if":
                job["if"] = value
            elif key == "continue-on-error":
                job[key] = True
            elif value:
                job["needs"] = [value]
        elif key == "needs" and (m := re.fullmatch(r"      - ([A-Za-z_][A-Za-z0-9_-]*)", line)):
            job["needs"].append(m[1])
        elif re.match(r"    \S", line):
            key = None
    return jobs


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

    def test_each_job_runs_on_its_flag(self):
        jobs = workflow_jobs()
        self.assertEqual(set(jobs), set(JOBS))
        self.assertFalse([n for n, j in jobs.items() if j["continue-on-error"]])
        for name, flag in JOBS.items():
            with self.subTest(job=name):
                if flag is None:
                    self.assertIn(jobs[name]["if"], (None, "always()"))
                else:
                    self.assertIn(flag, EVERY)
                    self.assertEqual(jobs[name]["if"], f"needs.changes.outputs.{flag} != 'false'")
                    self.assertEqual(jobs[name]["needs"], ["changes"])

    def test_the_gate_needs_every_job(self):
        jobs = workflow_jobs()
        self.assertEqual(jobs["gate"]["if"], "always()")
        self.assertEqual(set(jobs["gate"]["needs"]), set(jobs) - {"gate"})

    def test_an_unknown_argument_is_refused(self):
        for args in (["--al"], ["--all", "extra"], ["-"], [""], [""]):
            with self.subTest(args=args):
                proc = subprocess.run(
                    [SCRIPT, *args], input="", capture_output=True, text=True
                )
                self.assertEqual(proc.returncode, 2)
                self.assertEqual(proc.stdout, "")

    def test_all_and_an_empty_change(self):
        self.assertEqual(areas("--all")[0], EVERY)
        self.assertEqual(selected(), set())

    def test_a_change_to_ci_runs_everything(self):
        for path in (
            ".github/workflows/release.yml",
            ".github/actions/rust-toolchain/action.yml",
            "Makefile",
            "rust-toolchain.toml",
            "scripts/ci-areas.sh",
            "scripts/affected_crates.py",
        ):
            with self.subTest(path=path):
                self.assertEqual(selected(path), EVERY)

    def test_local_ignores_the_run_everything_rule(self):
        self.assertEqual(areas("--local", paths=["Makefile"])[0], set())
        self.assertEqual(areas("--local", paths=["docs/ui/notes.md"])[0], {"site"})
        self.assertEqual(areas("--local", paths=["scripts/package.sh"])[0], {"scripts"} | PACKAGE)

    def test_each_path_selects_its_areas(self):
        for path, expected in CASES:
            with self.subTest(path=path):
                self.assertEqual(selected(path), expected)

    def test_a_change_selects_the_union_of_its_paths(self):
        self.assertEqual(
            selected("README.md", "docs/ui/notes.md", "site/index.html"), {"site"}
        )
        self.assertEqual(
            selected("src-tauri/tests/ipc.rs", "scripts/package.sh"),
            RUST | PACKAGE | {"ui"},
        )


if __name__ == "__main__":
    unittest.main()
