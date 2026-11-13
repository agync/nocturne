"""Tests for the bundle overlays and scripts/package.sh --dry-run."""

import json
import os
import subprocess
import unittest

import bundle_config

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)


def dry_run(*args):
    return subprocess.run(
        [os.path.join(HERE, "package.sh"), "--dry-run", *args],
        capture_output=True,
        text=True,
        cwd=ROOT,
    )


class Merge(unittest.TestCase):
    def test_objects_merge_and_arrays_replace(self):
        merged = bundle_config.merge({"a": {"b": 1, "c": [1, 2]}}, {"a": {"c": [3]}})
        self.assertEqual(merged, {"a": {"b": 1, "c": [3]}})

    def test_null_removes_a_key(self):
        self.assertEqual(bundle_config.merge({"a": 1, "b": 2}, {"a": None}), {"b": 2})


class Overlays(unittest.TestCase):
    def test_both_files_parse(self):
        for name in ("linux", "windows", "macos"):
            path = os.path.join(ROOT, "src-tauri", "bundle", f"{name}.json")
            with open(path, encoding="utf-8") as handle:
                json.load(handle)

    def test_no_version_in_the_base_configuration(self):
        base = bundle_config.load(os.path.join(ROOT, "src-tauri", "tauri.conf.json"))
        self.assertNotIn("version", base)

    def test_linux_resources_point_at_the_verified_folder(self):
        merged = bundle_config.effective("linux")
        self.assertEqual(bundle_config.problems("linux", merged), [])
        self.assertEqual(merged["mainBinaryName"], "nocturne")
        self.assertEqual(
            sorted(merged["bundle"]["targets"]), ["appimage", "deb", "rpm"]
        )

    def test_windows_resources_and_bootstrapper(self):
        merged = bundle_config.effective("windows")
        self.assertEqual(bundle_config.problems("windows", merged), [])
        self.assertEqual(merged["bundle"]["windows"]["webviewInstallMode"]["type"], "embedBootstrapper")
        self.assertEqual(merged["bundle"]["windows"]["nsis"]["installMode"], "currentUser")
        self.assertEqual(sorted(merged["bundle"]["targets"]), ["msi", "nsis"])

    def test_macos_resources_icon_and_minimum_version(self):
        merged = bundle_config.effective("macos")
        self.assertEqual(bundle_config.problems("macos", merged), [])
        bundle = merged["bundle"]
        self.assertEqual(sorted(bundle["targets"]), ["app", "dmg"])
        self.assertIn("icons/icon.icns", bundle["icon"])
        self.assertTrue(os.path.isfile(os.path.join(ROOT, "src-tauri", "icons", "icon.icns")))
        self.assertEqual(bundle["macOS"]["signingIdentity"], "-")
        self.assertRegex(bundle["macOS"]["minimumSystemVersion"], r"^\d+\.\d+$")
        entitlements = os.path.join(ROOT, "src-tauri", bundle["macOS"]["entitlements"])
        self.assertTrue(os.path.isfile(entitlements))

    def test_the_microphone_sentence_is_plain(self):
        path = os.path.join(ROOT, "src-tauri", "Info.plist")
        with open(path, encoding="utf-8") as handle:
            text = handle.read()
        self.assertIn("<key>NSMicrophoneUsageDescription</key>", text)
        self.assertIn("Nocturne listens only while you hold to talk.", text)

    def test_licence_ships_in_every_package_and_the_engine_libraries_are_declared(self):
        for kind in ("linux", "windows", "macos"):
            self.assertEqual(bundle_config.effective(kind)["bundle"]["resources"]["../LICENSE"], "LICENSE")
        deb = bundle_config.effective("linux")["bundle"]["linux"]["deb"]["depends"]
        rpm = bundle_config.effective("linux")["bundle"]["linux"]["rpm"]["depends"]
        self.assertEqual(sorted(deb), sorted(set(deb)))
        for name in ("libssl3", "libzstd1", "libdrm-amdgpu1", "libayatana-appindicator3-1"):
            self.assertIn(name, deb)
        for name in ("openssl-libs", "libzstd", "libdrm", "libayatana-appindicator-gtk3"):
            self.assertIn(name, rpm)

    def test_a_different_version_is_refused(self):
        merged = bundle_config.effective("linux")
        merged["version"] = "9.9.9"
        self.assertTrue(bundle_config.problems("linux", merged))


class DryRun(unittest.TestCase):
    def test_linux_dry_run_prints_config_and_steps(self):
        proc = dry_run()
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn("../target/package/lemonade/linux-x86_64/", proc.stdout)
        self.assertIn(f"version {bundle_config.workspace_version()}", proc.stdout)
        self.assertIn("--bundles appimage,deb,rpm", proc.stdout)
        self.assertIn("fetch and verify lemonade", proc.stdout)

    def test_windows_dry_run_prints_windows_resources(self):
        proc = dry_run("--config", "windows")
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn("../target/package/lemonade/windows-x86_64/", proc.stdout)
        self.assertNotIn("linux-x86_64", proc.stdout)

    def test_macos_dry_run_prints_macos_resources_and_the_assembly_step(self):
        proc = dry_run("--config", "macos")
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn("../target/package/lemonade/macos-aarch64/", proc.stdout)
        self.assertIn("scripts/macos-app.sh", proc.stdout)
        self.assertNotIn("linux-x86_64", proc.stdout)

    def test_macos_build_is_refused_off_apple_silicon(self):
        if subprocess.run(["uname", "-sm"], capture_output=True, text=True).stdout.strip() == "Darwin arm64":
            self.skipTest("this is the machine that builds it")
        proc = subprocess.run(
            [os.path.join(HERE, "package.sh"), "--config", "macos"],
            capture_output=True,
            text=True,
            cwd=ROOT,
        )
        self.assertNotEqual(proc.returncode, 0)
        self.assertIn("Apple Silicon", proc.stderr)

    def test_windows_build_is_refused_here(self):
        proc = subprocess.run(
            [os.path.join(HERE, "package.sh"), "--config", "windows"],
            capture_output=True,
            text=True,
            cwd=ROOT,
        )
        self.assertNotEqual(proc.returncode, 0)
        self.assertIn("Windows machine", proc.stderr)


if __name__ == "__main__":
    unittest.main()
