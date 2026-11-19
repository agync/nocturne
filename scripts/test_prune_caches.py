"""Tests for scripts/prune_caches.py: which cache generations are kept."""

import os
import subprocess
import sys
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
SCRIPT = os.path.join(HERE, "prune_caches.py")

D0 = "2026-11-17T00:00:00Z"
D1 = "2026-11-18T00:00:00Z"
D2 = "2026-11-19T00:00:00Z"
NOW = "2026-11-20T00:00:00Z"


def row(cache_id, ref, key, created, accessed=None):
    return f"{cache_id}\t{ref}\t{key}\t{created}\t{accessed or created}\n"


def main_cache(cache_id, key, created, accessed=None):
    return row(cache_id, "refs/heads/main", key, created, accessed)


def rust(name, platform, environment, lock):
    return f"v0-rust-{name}-{platform}-{environment}-{lock}"


def run(text, args=("--now", NOW)):
    return subprocess.run(
        [sys.executable, "-I", SCRIPT, *args],
        input=text,
        capture_output=True,
        text=True,
    )


class Prune(unittest.TestCase):
    def test_keeps_the_newest_of_each_rust_family(self):
        text = "".join(
            (
                main_cache(1, rust("rust", "Linux-x64", "61ac8baf", "aaaaaaaa"), D1),
                # A second environment hash is the same job and platform, and older.
                main_cache(2, rust("rust", "Linux-x64", "61ac8baf", "bbbbbbbb"), D0),
                main_cache(3, rust("rust", "Linux-x64", "026af06a", "cccccccc"), D2),
                main_cache(4, rust("rust-platforms", "Darwin-arm64", "6390ed88", "dddddddd"), D0),
                main_cache(5, rust("rust-platforms", "Darwin-arm64", "6390ed88", "eeeeeeee"), D1),
                main_cache(
                    6,
                    rust("lint-rust-lint-platforms", "Windows_NT-x64", "7a660b75", "ffffffff"),
                    D0,
                ),
                main_cache(7, rust("package-linux", "Linux-x64", "026af06a", "11111111"), D0),
                main_cache(
                    8, rust("package-macos", "Darwin-arm64", "991976d2", "22222222"), D0
                ),
                main_cache(9, rust("package-windows", "Windows_NT-x64", "7a660b75", "33333333"), D0),
            )
        )
        proc = run(text)
        self.assertEqual(proc.returncode, 0)
        self.assertEqual(
            proc.stdout,
            "1\t" + rust("rust", "Linux-x64", "61ac8baf", "aaaaaaaa") + "\n"
            "2\t" + rust("rust", "Linux-x64", "61ac8baf", "bbbbbbbb") + "\n"
            "4\t" + rust("rust-platforms", "Darwin-arm64", "6390ed88", "dddddddd") + "\n",
        )

    def test_keeps_the_newest_of_each_other_family(self):
        text = "".join(
            (
                main_cache(10, "node-cache-Linux-x64-npm-" + "4" * 64, D1),
                main_cache(11, "node-cache-Linux-x64-npm-" + "5" * 64, D2),
                main_cache(12, "node-cache-macOS-arm64-npm-" + "6" * 64, D2),
                main_cache(13, "setup-go-Linux-x64-ubuntu24-go-1.27.3-" + "7" * 64, D2),
                main_cache(14, "pre-commit-Linux-" + "8" * 64, D2),
                main_cache(15, "lemonade-windows-x86_64-" + "9" * 64, D1),
                main_cache(16, "lemonade-windows-x86_64-" + "a" * 64, D2),
            )
        )
        proc = run(text)
        self.assertEqual(proc.returncode, 0)
        self.assertEqual(
            proc.stdout,
            "10\tnode-cache-Linux-x64-npm-" + "4" * 64 + "\n"
            "15\tlemonade-windows-x86_64-" + "9" * 64 + "\n",
        )

    def test_an_older_generation_in_use_is_kept(self):
        text = "".join(
            (
                main_cache(
                    40,
                    rust("rust", "Linux-x64", "61ac8baf", "aaaaaaaa"),
                    D0,
                    "2026-11-19T23:55:00Z",
                ),
                main_cache(41, rust("rust", "Linux-x64", "61ac8baf", "bbbbbbbb"), D1),
            )
        )
        proc = run(text)
        self.assertEqual(proc.returncode, 0)
        self.assertEqual(proc.stdout, "")

    def test_leaves_a_pull_requests_caches_alone(self):
        text = "".join(
            (
                main_cache(20, rust("rust", "Linux-x64", "61ac8baf", "aaaaaaaa"), D1),
                row(
                    21,
                    "refs/pull/114/merge",
                    rust("rust", "Linux-x64", "61ac8baf", "bbbbbbbb"),
                    D2,
                ),
                row(22, "refs/pull/114/merge", "node-cache-Linux-x64-npm-" + "b" * 64, D0),
            )
        )
        proc = run(text)
        self.assertEqual(proc.returncode, 0)
        self.assertEqual(proc.stdout, "")

    def test_nothing_to_delete(self):
        proc = run(main_cache(30, rust("rust", "Linux-x64", "61ac8baf", "aaaaaaaa"), D1))
        self.assertEqual(proc.returncode, 0)
        self.assertEqual(proc.stdout, "")

    def test_an_unrecognised_rust_key_fails(self):
        proc = run(main_cache(50, "v1-rust-rust-macos-x64-61ac8baf-aaaaaaaa", D0))
        self.assertEqual(proc.returncode, 1)
        self.assertEqual(proc.stdout, "")

    def test_a_naive_now_is_utc(self):
        text = "".join(
            (
                main_cache(51, rust("rust", "Linux-x64", "61ac8baf", "aaaaaaaa"), D1),
                main_cache(52, rust("rust", "Linux-x64", "61ac8baf", "bbbbbbbb"), D2),
            )
        )
        proc = run(text, ["--now", "2026-11-20T00:00:00"])
        self.assertEqual(proc.returncode, 0)
        self.assertEqual(
            proc.stdout, "51\t" + rust("rust", "Linux-x64", "61ac8baf", "aaaaaaaa") + "\n"
        )

    def test_a_now_that_is_not_a_date_is_refused(self):
        proc = run("", ["--now", "yesterday"])
        self.assertEqual(proc.returncode, 2)
        self.assertEqual(proc.stdout, "")

    def test_a_line_that_is_not_a_cache_fails(self):
        proc = run("1\trefs/heads/main\ta-key\n")
        self.assertEqual(proc.returncode, 1)
        self.assertEqual(proc.stdout, "")

    def test_an_argument_is_refused(self):
        proc = run("", ["--all"])
        self.assertEqual(proc.returncode, 2)
        self.assertEqual(proc.stdout, "")

    def test_a_now_without_a_date_is_refused(self):
        proc = run("", ["--now"])
        self.assertEqual(proc.returncode, 2)
        self.assertEqual(proc.stdout, "")


if __name__ == "__main__":
    unittest.main()
