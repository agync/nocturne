"""Tests for scripts/check_tree.py and scripts/check-package.sh on fixture trees."""

import hashlib
import sys
import os
import plistlib
import shutil
import struct
import subprocess
import tempfile
import unittest
import zlib

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)
HOME = "/" + "home" + "/someone"
BUILDER = "/" + "home" + "/builder"


def png(size):
    def chunk(kind, body):
        crc = zlib.crc32(kind + body) & 0xFFFFFFFF
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", crc)

    raw = b"".join(b"\0" + b"\0\0\0\0" * size for _ in range(size))
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw))
        + chunk(b"IEND", b"")
    )


def write(path, data, mode=0o644):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb" if isinstance(data, bytes) else "w") as handle:
        handle.write(data)
    os.chmod(path, mode)


def make_tree(root, lemond_mode=0o755, exec_line="nocturne"):
    write(os.path.join(root, "usr/bin/nocturne"), b"\x7fELF\0", 0o755)
    lib = os.path.join(root, "usr/lib/Nocturne")
    write(os.path.join(lib, "lemonade/lemond"), b"\x7fELF\0engine", lemond_mode)
    write(os.path.join(lib, "LICENSE"), "Apache")
    write(os.path.join(lib, "lemonade/LICENSE"), "Apache")
    write(os.path.join(lib, "lemonade/resources/defaults.json"), "{}")
    write(os.path.join(lib, "notices/THIRD-PARTY-NOTICES.txt"), "notices")
    write(
        os.path.join(root, "usr/share/applications/nocturne.desktop"),
        f"[Desktop Entry]\nName=Nocturne\nExec={exec_line}\nType=Application\n",
    )
    for size in (32, 128, 256):
        write(os.path.join(root, f"usr/share/icons/hicolor/{size}x{size}/apps/nocturne.png"), png(size))
    return lib


MAC_BUILDER = "/" + "Users" + "/builder"


def make_app(root, **plist):
    """A Nocturne.app tree: returns the Resources folder."""
    info = {
        "CFBundleIdentifier": "app.nocturne.desktop",
        "CFBundleExecutable": "Nocturne",
        "CFBundleIconFile": "icon",
        "CFBundleShortVersionString": "0.1.0",
        "LSMinimumSystemVersion": "26.0",
        "NSMicrophoneUsageDescription": "Nocturne listens only while you hold to talk.",
    }
    info.update(plist)
    info = {k: v for k, v in info.items() if v is not None}
    write(os.path.join(root, "Contents/Info.plist"), plistlib.dumps(info))
    write(os.path.join(root, "Contents/MacOS/Nocturne"), b"\xcf\xfa\xed\xfe\0", 0o755)
    res = os.path.join(root, "Contents/Resources")
    write(os.path.join(res, "icon.icns"), b"icns")
    write(os.path.join(res, "LICENSE"), "Apache")
    write(os.path.join(res, "lemonade/lemond"), b"\xcf\xfa\xed\xfe\0engine", 0o755)
    write(os.path.join(res, "lemonade/LICENSE"), "Apache")
    write(os.path.join(res, "notices/THIRD-PARTY-NOTICES.txt"), "notices")
    return res


def run_check(root, *extra):
    return subprocess.run(
        [os.path.join(HERE, "check-package.sh"), "--no-manifest", *extra, root],
        capture_output=True,
        text=True,
        cwd=ROOT,
        env=dict(os.environ, NOCTURNE_BUILDER_HOME=BUILDER),
    )


def run_tree(root, *args):
    """Run check_tree directly (to choose the package kind) and return (code, output)."""
    import contextlib
    import io

    import check_tree

    out = io.StringIO()
    with contextlib.redirect_stdout(out):
        code = check_tree.main([root, "--builder-home", BUILDER, *args])
    return code, out.getvalue()


class FixtureTrees(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, self.tmp, True)

    def test_a_good_tree_passes(self):
        make_tree(self.tmp)
        proc = run_check(self.tmp)
        self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
        self.assertIn("all checks passed", proc.stdout)

    def test_broken_tree_fails_with_both_reasons(self):
        make_tree(self.tmp, lemond_mode=0o644, exec_line=HOME + "/bin/nocturne")
        proc = run_check(self.tmp)
        self.assertEqual(proc.returncode, 1, proc.stdout + proc.stderr)
        self.assertIn("lemond is not executable", proc.stdout)
        self.assertIn("desktop entry", proc.stdout)
        self.assertIn("absolute or host path", proc.stdout)

    def test_stamp_file_and_agent_paths_fail(self):
        lib = make_tree(self.tmp)
        write(os.path.join(lib, "lemonade/.sha256"), "abc")
        write(os.path.join(self.tmp, "usr/share/.agents/x"), "x")
        proc = run_check(self.tmp)
        self.assertEqual(proc.returncode, 1)
        self.assertIn("stamp files", proc.stdout)
        self.assertIn("forbidden path parts", proc.stdout)

    def test_home_path_in_a_text_file_fails_but_binary_is_ignored(self):
        lib = make_tree(self.tmp)
        write(os.path.join(lib, "lemonade/blob.bin"), b"\0\0" + HOME.encode() + b"\0")
        self.assertEqual(run_check(self.tmp).returncode, 0)
        write(os.path.join(lib, "notices/leak.txt"), "built at " + HOME + "/src")
        proc = run_check(self.tmp)
        self.assertEqual(proc.returncode, 1)
        self.assertIn("text files naming a home directory", proc.stdout)

    def test_the_real_preview_binary_or_its_text_fails(self):
        lib = make_tree(self.tmp)
        self.assertEqual(run_check(self.tmp).returncode, 0)
        write(os.path.join(self.tmp, "usr/bin/nocturne-preview"), b"\x7fELF\0", 0o755)
        proc = run_check(self.tmp)
        self.assertEqual(proc.returncode, 1, proc.stdout)
        self.assertIn("development-only real preview", proc.stdout)
        os.remove(os.path.join(self.tmp, "usr/bin/nocturne-preview"))
        # The program itself, carrying the text the preview prints.
        write(os.path.join(self.tmp, "usr/bin/nocturne"), b"\x7fELF\0nocturne-preview-dev-only\0", 0o755)
        proc = run_check(self.tmp)
        self.assertEqual(proc.returncode, 1, proc.stdout)
        self.assertIn("nocturne", proc.stdout.split("development-only real preview")[1])
        write(os.path.join(self.tmp, "usr/bin/nocturne"), b"\x7fELF\0", 0o755)
        # The interface built for the preview, which keeps its token under a name of its own.
        write(os.path.join(lib, "ui/app.js"), 'sessionStorage.getItem("nocturne.preview.token")')
        proc = run_check(self.tmp)
        self.assertEqual(proc.returncode, 1, proc.stdout)
        self.assertIn("app.js", proc.stdout)

    def test_the_preview_interface_is_caught_in_the_plain_build_not_the_embedded_one(self):
        marker = b'sessionStorage.getItem("nocturne.preview.token")'
        dist = os.path.join(self.tmp, "dist")
        write(os.path.join(dist, "index.html"), "<html></html>")
        write(os.path.join(dist, "assets/app.js"), marker)
        script = os.path.join(HERE, "check_ui_dist.py")
        proc = subprocess.run([sys.executable, "-I", script, dist], capture_output=True, text=True)
        self.assertEqual(proc.returncode, 1, proc.stdout)
        self.assertIn("app.js", proc.stdout)
        # Embedded as the release build does, the text is no longer there to find.
        tree = os.path.join(self.tmp, "tree")
        make_tree(tree)
        write(os.path.join(tree, "usr/bin/nocturne"), b"\x7fELF\0" + zlib.compress(marker * 4), 0o755)
        self.assertEqual(run_check(tree).returncode, 0)
        write(os.path.join(dist, "assets/app.js"), "console.log(1)")
        proc = subprocess.run([sys.executable, "-I", script, dist], capture_output=True, text=True)
        self.assertEqual(proc.returncode, 0, proc.stdout)

    def test_missing_icon_size_fails(self):
        make_tree(self.tmp)
        os.remove(os.path.join(self.tmp, "usr/share/icons/hicolor/256x256/apps/nocturne.png"))
        proc = run_check(self.tmp)
        self.assertEqual(proc.returncode, 1)
        self.assertIn("icons missing", proc.stdout)

    def test_manifest_mismatch_fails(self):
        lib = make_tree(self.tmp)
        manifest = os.path.join(self.tmp, "..", os.path.basename(self.tmp) + ".manifest")
        self.addCleanup(lambda: os.path.exists(manifest) and os.remove(manifest))
        entries = []
        for rel in ("LICENSE", "lemond", "resources/defaults.json"):
            with open(os.path.join(lib, "lemonade", rel), "rb") as handle:
                entries.append(f"{hashlib.sha256(handle.read()).hexdigest()}  ./{rel}")
        with open(manifest, "w") as handle:
            handle.write("\n".join(entries) + "\n")
        good = subprocess.run(
            [os.path.join(HERE, "check-package.sh"), "--manifest", manifest, self.tmp],
            capture_output=True,
            text=True,
            cwd=ROOT,
        )
        self.assertEqual(good.returncode, 0, good.stdout + good.stderr)
        write(os.path.join(lib, "lemonade/resources/defaults.json"), "{\"changed\": 1}")
        bad = subprocess.run(
            [os.path.join(HERE, "check-package.sh"), "--manifest", manifest, self.tmp],
            capture_output=True,
            text=True,
            cwd=ROOT,
        )
        self.assertEqual(bad.returncode, 1)
        self.assertIn("changed resources/defaults.json", bad.stdout)


class AppBundles(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, self.tmp, True)

    def check(self, *extra):
        return run_tree(self.tmp, "--kind", "app", "--version", "0.1.0", *extra)

    def test_a_good_app_passes(self):
        make_app(self.tmp)
        code, out = self.check()
        self.assertEqual(code, 0, out)
        self.assertIn("microphone sentence: Nocturne listens only while you hold to talk.", out)
        self.assertIn("minimum macOS 26.0", out)

    def test_a_missing_microphone_sentence_fails(self):
        make_app(self.tmp, NSMicrophoneUsageDescription=None)
        code, out = self.check()
        self.assertEqual(code, 1)
        self.assertIn("NSMicrophoneUsageDescription is missing", out)

    def test_wrong_identifier_version_and_missing_minimum_fail(self):
        make_app(self.tmp, CFBundleIdentifier="com.example.x", CFBundleShortVersionString="9.9.9", LSMinimumSystemVersion=None)
        code, out = self.check()
        self.assertEqual(code, 1)
        self.assertIn("bundle identifier is 'com.example.x'", out)
        self.assertIn("bundle version '9.9.9' differs", out)
        self.assertIn("LSMinimumSystemVersion is missing", out)

    def test_the_icon_and_the_executable_must_exist(self):
        res = make_app(self.tmp, CFBundleExecutable="Other")
        os.remove(os.path.join(res, "icon.icns"))
        code, out = self.check()
        self.assertEqual(code, 1)
        self.assertIn("CFBundleExecutable 'Other' is not in Contents/MacOS", out)
        self.assertIn("CFBundleIconFile 'icon' is not in Contents/Resources", out)

    def test_a_mac_home_folder_in_a_binary_or_a_text_file_fails_but_not_inside_the_engine(self):
        res = make_app(self.tmp)
        write(os.path.join(res, "lemonade/blob.bin"), b"\0" + MAC_BUILDER.encode() + b"/x\0")
        write(os.path.join(res, "lemonade/note.txt"), MAC_BUILDER + "/x")
        self.assertEqual(self.check("--builder-home", MAC_BUILDER)[0], 0)
        write(os.path.join(self.tmp, "Contents/MacOS/Nocturne"), b"\xcf\xfa\xed\xfe\0" + MAC_BUILDER.encode() + b"/.cargo\0", 0o755)
        code, out = self.check("--builder-home", MAC_BUILDER)
        self.assertEqual(code, 1)
        self.assertIn("builder's home directory", out)
        write(os.path.join(res, "notices/THIRD-PARTY-NOTICES.txt"), "built in " + MAC_BUILDER + "/src")
        code, out = self.check()
        self.assertIn("naming a Mac home folder", out)

    def test_engine_files_must_match_the_manifest(self):
        res = make_app(self.tmp)
        manifest = os.path.join(self.tmp, "m.manifest")
        digest = hashlib.sha256(b"\xcf\xfa\xed\xfe\0engine").hexdigest()
        other = hashlib.sha256(b"Apache").hexdigest()
        with open(manifest, "w") as handle:
            handle.write(f"{digest}  ./lemond\n{other}  ./LICENSE\n")
        code, out = self.check("--manifest", manifest)
        self.assertEqual(code, 0, out)
        write(os.path.join(res, "lemonade/lemond"), b"tampered", 0o755)
        code, out = self.check("--manifest", manifest)
        self.assertEqual(code, 1)
        self.assertIn("changed lemond", out)


class Review(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, self.tmp, True)

    def manifest_for(self, lib):
        manifest = os.path.join(self.tmp, "lemonade.manifest")
        entries = []
        base = os.path.join(lib, "lemonade")
        for dirpath, _, names in os.walk(base):
            for name in names:
                path = os.path.join(dirpath, name)
                if os.path.islink(path):
                    continue
                with open(path, "rb") as handle:
                    entries.append(f"{hashlib.sha256(handle.read()).hexdigest()}  ./{os.path.relpath(path, base)}")
        with open(manifest, "w") as handle:
            handle.write("\n".join(entries) + "\n")
        return manifest

    def test_a_tampered_engine_with_the_same_code_sections_fails_on_an_appimage(self):
        lib = make_tree(self.tmp)
        shutil.copy(shutil.which("true"), os.path.join(lib, "lemonade/lemond"))
        os.chmod(os.path.join(lib, "lemonade/lemond"), 0o755)
        manifest = self.manifest_for(lib)
        code, out = run_tree(self.tmp, "--kind", "appimage", "--manifest", manifest)
        self.assertIn("match the verified manifest byte for byte", out)
        with open(os.path.join(lib, "lemonade/lemond"), "r+b") as handle:
            handle.seek(0x18)  # the entry point of the ELF header, outside every code section
            byte = handle.read(1)
            handle.seek(0x18)
            handle.write(bytes([byte[0] ^ 0xFF]))
        code, out = run_tree(self.tmp, "--kind", "appimage", "--manifest", manifest)
        self.assertEqual(code, 1)
        self.assertIn("changed lemond", out)

    def test_a_symlink_in_the_engine_folder_fails(self):
        lib = make_tree(self.tmp)
        manifest = self.manifest_for(lib)
        os.symlink("/etc/hostname", os.path.join(lib, "lemonade/link"))
        code, out = run_tree(self.tmp, "--manifest", manifest)
        self.assertEqual(code, 1)
        self.assertIn("symlink link", out)

    def test_the_builders_home_in_a_binary_fails_but_not_inside_the_engine(self):
        lib = make_tree(self.tmp)
        write(os.path.join(lib, "lemonade/blob.bin"), b"\0" + BUILDER.encode() + b"/x\0")
        self.assertEqual(run_tree(self.tmp)[0], 0)
        write(os.path.join(self.tmp, "usr/bin/nocturne"), b"\x7fELF\0" + BUILDER.encode() + b"/.cargo/registry\0", 0o755)
        code, out = run_tree(self.tmp)
        self.assertEqual(code, 1)
        self.assertIn("builder's home directory", out)

    def test_missing_nocturne_licence_fails(self):
        lib = make_tree(self.tmp)
        os.remove(os.path.join(lib, "LICENSE"))
        code, out = run_tree(self.tmp)
        self.assertEqual(code, 1)
        self.assertIn("own LICENSE is missing", out)

    def test_dependencies_must_cover_the_tray_and_the_engine(self):
        make_tree(self.tmp)
        req = os.path.join(self.tmp, "req.txt")
        with open(req, "w") as handle:
            handle.write("libayatana-appindicator3-1")
        code, out = run_tree(self.tmp, "--kind", "deb", "--requires", req)
        self.assertEqual(code, 1)
        self.assertIn("libssl3", out)
        with open(req, "w") as handle:
            handle.write("libayatana-appindicator3-1, libssl3, libzstd1, libdrm-amdgpu1")
        self.assertEqual(run_tree(self.tmp, "--kind", "deb", "--requires", req)[0], 0)

    def bundled(self, entry_license, listed=True, excluded=False):
        make_tree(self.tmp)
        usr_lib = os.path.join(self.tmp, "usr/lib")
        write(os.path.join(usr_lib, "libfoo.so.1"), b"\x7fELF")
        if excluded:
            write(os.path.join(usr_lib, "gtk-3.0/modules/libpk-gtk-module.so"), b"\x7fELF")
        files = "  libfoo.so.1\n" if listed else ""
        doc = f"Shared libraries\n\nfoo 1-1  ({entry_license})\n  source package: foo-1-1.fc44\n{files}"
        write(os.path.join(usr_lib, "Nocturne/notices/BUNDLED-LIBRARIES.txt"), doc)
        sources = os.path.join(self.tmp, "SOURCES.txt")
        write(sources, "foo-1-1.fc44\tfoo\t" + entry_license + "\n")
        return sources

    def test_bundled_libraries_pass_when_listed_and_permitted(self):
        sources = self.bundled("LGPL-2.1-or-later")
        code, out = run_tree(self.tmp, "--kind", "appimage", "--sources", sources)
        self.assertIn("pass the copyleft policy", out)
        self.assertIn("names every bundled source package", out)

    def test_bundled_gpl_library_fails(self):
        sources = self.bundled("GPL-2.0-or-later")
        code, out = run_tree(self.tmp, "--kind", "appimage", "--sources", sources)
        self.assertEqual(code, 1)
        self.assertIn("fail the copyleft policy", out)

    def test_unlisted_bundled_library_fails(self):
        sources = self.bundled("MIT", listed=False)
        code, out = run_tree(self.tmp, "--kind", "appimage", "--sources", sources)
        self.assertEqual(code, 1)
        self.assertIn("missing from BUNDLED-LIBRARIES.txt", out)

    def test_an_excluded_module_fails(self):
        sources = self.bundled("MIT", excluded=True)
        code, out = run_tree(self.tmp, "--kind", "appimage", "--sources", sources)
        self.assertEqual(code, 1)
        self.assertIn("modules the app does not use", out)

    def test_missing_sources_manifest_entry_fails(self):
        self.bundled("MIT")
        other = os.path.join(self.tmp, "OTHER.txt")
        write(other, "bar-1-1.fc44\tbar\tMIT\n")
        code, out = run_tree(self.tmp, "--kind", "appimage", "--sources", other)
        self.assertEqual(code, 1)
        self.assertIn("missing from the SOURCES manifest", out)


if __name__ == "__main__":
    unittest.main()
