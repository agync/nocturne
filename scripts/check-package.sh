#!/usr/bin/env bash
# Open a Nocturne package (AppImage, deb, rpm, a macOS .app or .dmg, or an unpacked tree) and
# inspect it.
#
# Usage: scripts/check-package.sh [--manifest FILE | --no-manifest] <artefact | directory>
#
# Checks the binary, the private engine (present, executable, byte for byte the verified bundle
# recorded in target/package/lemonade/<platform>.manifest), licences and notices, the desktop
# entry, icons, forbidden paths, host paths in text files and the builder's home folder in any
# file (binaries too), the package version and, for deb and rpm, the declared libraries. For an
# AppImage it also checks the bundled libraries against the copyleft policy and the SOURCES
# manifest. For Away from home it checks that no connection helper (cloudflared, nocturne-tailnet,
# or the retired tailscaled and tailscale) and no stand-in helper is inside, that the binary carries neither
# stand-in marker (helpers, or the online stand-in provider), that the binary embeds
# the repository's helpers.lock, and that every pinned version has its captured help text; with NOCTURNE_RELEASE=1
# it also refuses a placeholder pin, a pinned release older than 183 days and help text that was not
# captured from the pinned binary. Unpacks into a scratch folder and removes it. A macOS .app (or the one inside a
# .dmg, mounted read-only) is checked for its Info.plist (identifier, executable, icon, version,
# minimum system, microphone sentence) and, on a Mac, its code signature.
#   NOCTURNE_BUILDER_HOME   the home folder to look for (default $HOME)
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

manifest=""
manifest_set=0
sources=target/package/SOURCES.txt
while (($#)); do
    case "$1" in
        --manifest) manifest="${2:?--manifest needs a file}"; manifest_set=1; shift ;;
        --no-manifest) manifest=""; manifest_set=1 ;;
        -h | --help) sed -n '2,9p' "$0"; exit 0 ;;
        *) break ;;
    esac
    shift
done
[[ $# -eq 1 ]] || { printf 'usage: scripts/check-package.sh [--manifest FILE | --no-manifest] <artefact>\n' >&2; exit 2; }
input=$(realpath -- "$1")
[[ -e "$input" ]] || { printf 'not found: %s\n' "$1" >&2; exit 2; }
platform=linux-x86_64
case "$input" in *.app | *.app/ | *.dmg) platform=macos-aarch64 ;; esac
((manifest_set)) || manifest=target/package/lemonade/$platform.manifest
if [[ -n "$manifest" && ! -f "$manifest" ]]; then
    printf 'manifest not found: %s (run scripts/package.sh first, or pass --no-manifest)\n' "$manifest" >&2
    exit 2
fi

scratch=$(mktemp -d)
# A disk image is detached before the scratch folder goes, and the folder is only removed once
# it is: rm -rf must never walk into a mounted image. A busy mount is detached by force.
mounted() { mount | grep -qF " on $mnt_real ("; }
mnt_real=""
cleanup() {
    if [[ -n "$mnt_real" ]] && mounted; then
        hdiutil detach -quiet "$mnt_real" > /dev/null 2>&1 \
            || { sleep 1; hdiutil detach -force -quiet "$mnt_real" > /dev/null 2>&1; }
        if mounted; then
            printf 'could not detach %s; leaving %s in place\n' "$mnt_real" "$scratch" >&2
            return
        fi
    fi
    rm -rf "$scratch"
}
trap cleanup EXIT
version=$(python3 -I -c 'import sys; sys.path.insert(0, "scripts"); import bundle_config; print(bundle_config.workspace_version())')
root="$scratch/root"
mkdir -p "$root"
kind=tree
pkg_version=""
requires=""

if [[ -d "$input" ]]; then
    root="$input"
    if [[ "$input" == *.app ]]; then kind=app; fi
else
    case "$input" in
        *.dmg)
            kind=app
            mkdir "$scratch/mnt"
            mnt_real=$(cd "$scratch/mnt" && pwd -P)
            hdiutil attach -quiet -nobrowse -readonly -noverify -noautoopen -mountpoint "$mnt_real" "$input"
            root=$(find "$mnt_real" -maxdepth 1 -name '*.app' | head -1)
            [[ -n "$root" ]] || { printf 'no .app inside %s\n' "$input" >&2; exit 1; }
            pkg_version=$(basename "$input" | sed -n 's/^[^_]*_\([0-9][^_]*\)_.*/\1/p')
            ;;
        *.AppImage)
            kind=appimage
            cp -- "$input" "$scratch/app.AppImage"
            chmod +x "$scratch/app.AppImage"
            (cd "$scratch" && APPIMAGE_EXTRACT_AND_RUN=1 ./app.AppImage --appimage-extract > /dev/null)
            root="$scratch/squashfs-root"
            pkg_version=$(basename "$input" | sed -n 's/^[^_]*_\([0-9][^_]*\)_.*/\1/p')
            ;;
        *.deb)
            kind=deb
            (cd "$scratch" && ar x "$input")
            data=$(ls "$scratch"/data.tar.* | head -1)
            ctl=$(ls "$scratch"/control.tar.* | head -1)
            tar -xf "$data" -C "$root"
            mkdir "$scratch/control" && tar -xf "$ctl" -C "$scratch/control"
            pkg_version=$(sed -n 's/^Version: //p' "$scratch/control/control")
            sed -n 's/^Depends: //p' "$scratch/control/control" > "$scratch/requires"
            requires="$scratch/requires"
            ;;
        *.rpm)
            kind=rpm
            (cd "$root" && rpm2cpio "$input" | cpio -idm --quiet)
            pkg_version=$(rpm -qp --qf '%{VERSION}' "$input" 2> /dev/null)
            rpm -qpR "$input" > "$scratch/requires" 2> /dev/null
            requires="$scratch/requires"
            ;;
        *) printf 'unknown package type: %s\n' "$input" >&2; exit 2 ;;
    esac
fi

args=("$root" --kind "$kind" --builder-home "${NOCTURNE_BUILDER_HOME:-$HOME}")
[[ "$kind" == appimage && -f "$sources" ]] && args+=(--sources "$sources")
[[ "$kind" != tree ]] && args+=(--version "$version" --package-version "$pkg_version")
[[ -n "$manifest" ]] && args+=(--manifest "$manifest")
[[ -n "$requires" ]] && args+=(--requires "$requires")
printf 'checking %s (%s)\n' "$(basename "$input")" "$kind"
python3 -I scripts/check_tree.py "${args[@]}"
python3 -I - "$root" "${NOCTURNE_RELEASE:-0}" << 'PY'
import datetime
import os
import sys
import tomllib

root, release = sys.argv[1], sys.argv[2] == "1"
FORBIDDEN = {"cloudflared", "cloudflared.exe", "tailscaled", "tailscaled.exe", "tailscale", "tailscale.exe", "nocturne-tailnet", "nocturne-tailnet.exe"}
FAKE_MARKER = b"nocturne-fake-helpers-marker"
ONLINE_MARKER = b"nocturne-online-stand-in-marker"
SKIP_DIRS = {"lemonade", "engine", "models"}
failed = False


def good(message):
    print(f"ok    {message}")


def bad(message):
    global failed
    failed = True
    print(f"FAIL  {message}")


files = []
for base, dirs, names in os.walk(root):
    dirs[:] = [d for d in dirs if d not in SKIP_DIRS]
    files += [os.path.join(base, n) for n in names]

# No helper is ever inside the package: it is downloaded on consent and checked against the pin.
found = [f for f in files if os.path.basename(f) in FORBIDDEN or os.path.basename(f).startswith("fake-")]
if found:
    bad("a connection helper or a stand-in is packaged: " + ", ".join(os.path.relpath(f, root) for f in found[:5]))
else:
    good("no connection helper and no stand-in helper in the package")

with open("helpers.lock", "rb") as handle:
    lock_bytes = handle.read()
lock = tomllib.loads(lock_bytes.decode("utf-8"))
binaries = [f for f in files if os.path.basename(f) in ("nocturne", "nocturne.exe") and os.path.getsize(f) > 1 << 20]
marker_in = []
online_in = []
embeds = []
different = []
for path in binaries:
    data = open(path, "rb").read()
    if FAKE_MARKER in data:
        marker_in.append(path)
    if ONLINE_MARKER in data:
        online_in.append(path)
    if lock_bytes in data:
        embeds.append(path)
    elif b'kind = "cloudflared"' in data:
        different.append(path)
if marker_in:
    bad("a build with stand-in helpers was packaged: " + ", ".join(os.path.relpath(f, root) for f in marker_in))
else:
    good("no stand-in helper marker in the program")
if online_in:
    bad("a build with the online stand-in provider was packaged: " + ", ".join(os.path.relpath(f, root) for f in online_in))
else:
    good("no online stand-in marker in the program")
if binaries and different:
    bad("the program embeds a helpers.lock that differs from the repository's")
elif binaries and not embeds:
    bad("the program does not embed helpers.lock")
elif binaries:
    good("the program embeds the repository's helpers.lock")
else:
    print("note: no nocturne program found, so the embedded helpers.lock was not compared")

for helper in lock["helpers"]:
    kind, version = helper["kind"], helper["version"]
    folder = os.path.join("crates", "helpers", "fixtures", "help", kind, version)
    texts = sorted(n for n in os.listdir(folder) if n.endswith(".txt")) if os.path.isdir(folder) else []
    if texts:
        good(f"help text captured for {kind} {version} ({len(texts)} commands)")
    else:
        bad(f"no captured help text for {kind} {version} in {folder}")
    if release:
        placeholder = helper.get("placeholder") or any(
            set(a["sha256"]) == {"0"} for a in helper["assets"].values()
        )
        if placeholder:
            bad(f"{kind} {version} is still a placeholder pin")
        try:
            released = datetime.date.fromisoformat(helper.get("released_at", ""))
        except ValueError:
            bad(f"{kind} {version} has no valid released_at date")
        else:
            age = (datetime.date.today() - released).days
            if age > 183:
                bad(f"{kind} {version} was released {age} days ago; pins must be younger than 183 days")
            else:
                good(f"{kind} {version} released {age} days ago")
        stale = [n for n in texts if "not yet the pinned binary" in open(os.path.join(folder, n), encoding="utf-8").read()]
        if stale:
            bad(f"help text for {kind} {version} was not captured from the pinned binary: {', '.join(stale)}")
sys.exit(1 if failed else 0)
PY
if [[ "$kind" == app && "$(uname -s)" == Darwin ]]; then
    codesign --verify --deep --strict "$root"
    printf 'ok    code signature verifies (%s)\n' "$(codesign -dv "$root" 2>&1 | sed -n 's/^Signature=//p;s/^Authority=//p' | head -1)"
elif [[ "$kind" == app ]]; then
    printf 'note: the code signature is only checked on a Mac\n'
fi
