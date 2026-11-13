#!/usr/bin/env bash
# Open a Nocturne package (AppImage, deb or rpm, or an unpacked tree) and inspect it.
#
# Usage: scripts/check-package.sh [--manifest FILE | --no-manifest] <artefact | directory>
#
# Checks the binary, the private engine (present, executable, byte for byte the verified bundle
# recorded in target/package/lemonade/<platform>.manifest), licences and notices, the desktop
# entry, icons, forbidden paths, host paths in text files and the builder's home folder in any
# file (binaries too), the package version and, for deb and rpm, the declared libraries. For an
# AppImage it also checks the bundled libraries against the copyleft policy and the SOURCES
# manifest. Unpacks into a scratch folder and removes it.
#   NOCTURNE_BUILDER_HOME   the home folder to look for (default $HOME)
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

manifest=target/package/lemonade/linux-x86_64.manifest
sources=target/package/SOURCES.txt
while (($#)); do
    case "$1" in
        --manifest) manifest="${2:?--manifest needs a file}"; shift ;;
        --no-manifest) manifest="" ;;
        -h | --help) sed -n '2,9p' "$0"; exit 0 ;;
        *) break ;;
    esac
    shift
done
[[ $# -eq 1 ]] || { printf 'usage: scripts/check-package.sh [--manifest FILE | --no-manifest] <artefact>\n' >&2; exit 2; }
input=$(realpath -- "$1")
[[ -e "$input" ]] || { printf 'not found: %s\n' "$1" >&2; exit 2; }
if [[ -n "$manifest" && ! -f "$manifest" ]]; then
    printf 'manifest not found: %s (run scripts/package.sh first, or pass --no-manifest)\n' "$manifest" >&2
    exit 2
fi

scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
version=$(python3 -I -c 'import sys; sys.path.insert(0, "scripts"); import bundle_config; print(bundle_config.workspace_version())')
root="$scratch/root"
mkdir -p "$root"
kind=tree
pkg_version=""
requires=""

if [[ -d "$input" ]]; then
    root="$input"
else
    case "$input" in
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
