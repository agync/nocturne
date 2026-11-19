"""Tests for scripts/check-changed.sh: which checks a list of changed paths picks."""

import os
import subprocess
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
SCRIPT = os.path.join(HERE, "check-changed.sh")


def dry_run(*paths, tests=None):
    env = {k: v for k, v in os.environ.items() if k != "TESTS"}
    if tests:
        env["TESTS"] = tests
    with tempfile.NamedTemporaryFile("w", suffix=".txt") as f:
        f.write("".join(p + "\n" for p in paths))
        f.flush()
        proc = subprocess.run(
            [SCRIPT, "--dry-run", "--paths", f.name],
            capture_output=True,
            text=True,
            check=True,
            env=env,
        )
    targets = next(l for l in proc.stdout.splitlines() if l.startswith("targets: "))
    return targets.removeprefix("targets: ").split(), proc.stdout


class CheckChanged(unittest.TestCase):
    def test_a_rust_change_picks_the_rust_checks(self):
        targets, _ = dry_run("crates/engine/src/lib.rs")
        self.assertIn("rust/lint", targets)
        self.assertIn("contract/check", targets)

    def test_a_docs_change_skips_rust(self):
        targets, out = dry_run("docs/contribution/guide.md")
        self.assertEqual(targets, ["docs/check", "site/strings"])
        self.assertIn("pre-commit run --files", out)

    def test_the_makefile_alone_runs_no_bulk_target(self):
        targets, _ = dry_run("Makefile")
        self.assertEqual(targets, ["none"])

    def test_nothing_changed_runs_nothing(self):
        targets, out = dry_run()
        self.assertEqual(targets, ["none"])
        self.assertNotIn("pre-commit", out)

    def test_tests_pass_through(self):
        _, out = dry_run("README.md", tests="cargo test -p x y::")
        self.assertIn("+ bash -c cargo test -p x y::", out)

    def test_an_unknown_argument_is_refused(self):
        proc = subprocess.run([SCRIPT, "--bogus"], capture_output=True, text=True)
        self.assertEqual(proc.returncode, 2)


if __name__ == "__main__":
    unittest.main()
