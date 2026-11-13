#!/usr/bin/env bash
# Build Nocturne's installable packages with the pinned private engine inside.
#
# Usage: scripts/package.sh [--dry-run] [--config linux|windows|macos] [--inputs-only]
#                           [appimage|deb|rpm|nsis|msi|app|dmg ...]
#
# Everything lands under target/package/: the verified Lemonade bundle, its sha256 manifest, the
# third-party notices, the packages and SHA256SUMS. Nothing is installed, pushed or uploaded.
# The only network use is the developer-time download of the pinned Lemonade archive (checked
# against lemonade.lock) and of the packaging tools the Tauri CLI fetches into its own cache.
# --dry-run prints the merged configuration and the steps without building or downloading.
#
# macOS (Apple Silicon, built on a Mac) needs no Node there: the app is assembled by
# scripts/macos-app.sh. The interface (ui/dist) and the notices come from a machine with Node:
# run `scripts/package.sh --config macos --inputs-only` there and copy ui/dist and
# target/package/notices to the Mac; with Node present the Mac builds them itself.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
source scripts/sha256.sh

kind=linux
dry=0
inputs_only=0
targets=()
while (($#)); do
    case "$1" in
        --dry-run) dry=1 ;;
        --inputs-only) inputs_only=1 ;;
        --config) kind="${2:?--config needs linux, windows or macos}"; shift ;;
        appimage | deb | rpm | nsis | msi | app | dmg) targets+=("$1") ;;
        -h | --help) sed -n '2,15p' "$0"; exit 0 ;;
        *) printf 'unknown argument: %s\n' "$1" >&2; exit 2 ;;
    esac
    shift
done
[[ "$kind" == linux || "$kind" == windows || "$kind" == macos ]] || { printf 'unknown config: %s\n' "$kind" >&2; exit 2; }
platform=linux-x86_64
rust_target=x86_64-unknown-linux-gnu
defaults=(appimage deb rpm)
if [[ "$kind" == windows ]]; then
    platform=windows-x86_64
    rust_target=x86_64-pc-windows-msvc
    defaults=(nsis msi)
elif [[ "$kind" == macos ]]; then
    platform=macos-aarch64
    rust_target=aarch64-apple-darwin
    defaults=(app dmg)
fi
((${#targets[@]})) || targets=("${defaults[@]}")
bundles=$(IFS=,; printf '%s' "${targets[*]}")
pkg=target/package
version=$(python3 -I -c 'import sys; sys.path.insert(0, "scripts"); import bundle_config; print(bundle_config.workspace_version())')

say() { printf '==> %s\n' "$*"; }

if [[ -n "$(git status --porcelain 2> /dev/null)" ]]; then
    printf 'note: the working tree has uncommitted changes; the packages will include them\n' >&2
fi

say "configuration ($kind, version $version)"
python3 -I scripts/bundle_config.py "$kind"

if ((dry)) && [[ "$kind" == macos ]]; then
    say "dry run: steps that would run"
    cat <<STEPS
fetch and verify lemonade $platform into $pkg/lemonade/$platform (manifest $pkg/lemonade/$platform.manifest)
interface: npm --prefix ui ci && npm --prefix ui run build (or the ui/dist copied from a machine with Node)
scripts/notices.py --target $rust_target --lemonade $pkg/lemonade/$platform (or the notices copied the same way)
cargo build --release -p nocturne-app --features custom-protocol
scripts/macos-app.sh --manifest $pkg/lemonade/$platform.manifest target/release/nocturne-app $pkg/lemonade/$platform $pkg/notices $pkg/macos
copy the .dmg to $pkg and write $pkg/SHA256SUMS
STEPS
    exit 0
fi

if ((dry)); then
    say "dry run: steps that would run"
    cat <<STEPS
fetch and verify lemonade $platform into $pkg/lemonade/$platform (manifest $pkg/lemonade/$platform.manifest)
npm --prefix ui ci && npm --prefix ui run build
scripts/notices.py --target $rust_target --lemonade $pkg/lemonade/$platform
(cd src-tauri && ../ui/node_modules/.bin/tauri build --config bundle/$kind.json --bundles $bundles)
for AppImage: remove unused modules, restore the verified engine files, add the bundled libraries' licences, pack again
copy artefacts to $pkg and write $pkg/SHA256SUMS (with SOURCES.txt, the source packages of the bundled libraries)
STEPS
    exit 0
fi

host=$(uname -s)-$(uname -m)
if [[ "$kind" == windows ]]; then
    case "$host" in
        MINGW*-x86_64 | MSYS*-x86_64) ;;
        *) printf 'Windows installers are built on a Windows machine (Git Bash): see docs/contribution/building.md\n' >&2; exit 2 ;;
    esac
elif [[ "$kind" == macos ]]; then
    if [[ "$host" != Darwin-arm64 ]] && ((!inputs_only)); then
        printf 'the Mac package is built on Apple Silicon; --inputs-only prepares what it needs anywhere\n' >&2
        exit 2
    fi
elif [[ "$host" != Linux-x86_64 ]]; then
    printf 'Linux packages are built on Linux x86_64\n' >&2
    exit 2
fi

# Fetch the pinned archive fresh, verify it against lemonade.lock, extract it and record a
# sha256 of every file. The stamp scripts/fetch-lemonade.sh leaves in vendor/ is never trusted.
field() { sed -n "s/^${platform} = .*${1} = \"\([^\"]*\)\".*/\1/p" lemonade.lock; }
base_url=$(sed -n 's/^base_url = "\(.*\)"/\1/p' lemonade.lock)
file=$(field file)
sum=$(field sha256)
[[ -n "$file" && -n "$sum" ]] || { printf 'no asset for %s in lemonade.lock\n' "$platform" >&2; exit 2; }

mkdir -p "$pkg/download"
archive="$pkg/download/$file"
verified() { [[ -f "$archive" && "$(sha256_of "$archive")" == "$sum" ]]; }
if ! verified; then
    say "download $file"
    curl --fail --location --silent --show-error -o "$archive.part" "$base_url/$file"
    mv "$archive.part" "$archive"
fi
verified || { printf 'sha256 mismatch for %s\n' "$file" >&2; exit 1; }
say "lemonade archive verified ($sum)"

work=$(mktemp -d "$pkg/extract.XXXXXX")
trap 'rm -rf "$work"' EXIT
case "$file" in
    *.zip) unzip -q "$archive" -d "$work" ;;
    *) tar -xzf "$archive" -C "$work" ;;
esac
# The archive must hold exactly one top-level folder and no symlinks: the manifest records
# regular files only, so a link would be shipped unverified.
entries=$(find "$work" -mindepth 1 -maxdepth 1 | wc -l)
inner=$(find "$work" -mindepth 1 -maxdepth 1 -type d | head -1)
if [[ "$entries" -ne 1 || -z "$inner" ]]; then
    printf 'the Lemonade archive must hold exactly one top-level folder (found %s entries)\n' "$entries" >&2
    exit 1
fi
if [[ -n "$(find "$inner" -type l | head -1)" ]]; then
    printf 'the Lemonade archive contains symlinks: %s\n' "$(find "$inner" -type l | head -3 | tr '\n' ' ')" >&2
    exit 1
fi
rm -rf "$pkg/lemonade/$platform"
mkdir -p "$pkg/lemonade/$platform"
cp -a "$inner/." "$pkg/lemonade/$platform/"
rm -f "$pkg/lemonade/$platform/.sha256"
[[ -f "$pkg/lemonade/$platform/lemond" ]] && chmod +x "$pkg/lemonade/$platform/lemond"
sha256_manifest "$pkg/lemonade/$platform" > "$pkg/lemonade/$platform.manifest"
say "lemonade manifest: $(wc -l < "$pkg/lemonade/$platform.manifest") files"

if [[ "$kind" == macos ]] && ! command -v npm > /dev/null 2>&1; then
    [[ -f ui/dist/index.html && -f "$pkg/notices/THIRD-PARTY-NOTICES.txt" ]] || {
        printf 'no Node here: copy ui/dist and %s/notices from a machine that ran\n' "$pkg" >&2
        printf '  scripts/package.sh --config macos --inputs-only\n' >&2
        exit 1
    }
    say "interface and notices: the copies already here"
else
    say "interface"
    npm --prefix ui ci
    npm --prefix ui run build

    say "third-party notices"
    rm -rf "$pkg/notices"
    python3 -I scripts/notices.py --target "$rust_target" --lemonade "$pkg/lemonade/$platform"
fi
if ((inputs_only)); then
    say "inputs ready: ui/dist, $pkg/notices and $pkg/lemonade/$platform"
    exit 0
fi

# Run from src-tauri, as the Makefile's dev target does: the CLI runs the interface build hook
# relative to the project it finds there.
build() {
    (cd src-tauri && ../ui/node_modules/.bin/tauri build --config "bundle/$kind.json" --bundles "$1" "${@:2}")
}
export NO_STRIP=true APPIMAGE_EXTRACT_AND_RUN=1

# Keep the builder's folders out of the binary: panic messages and debug paths would otherwise
# carry the home folder of whoever built it. The most specific prefix goes last, which wins.
cargo_home=${CARGO_HOME:-$HOME/.cargo}
rustup_home=${RUSTUP_HOME:-$HOME/.rustup}
remaps=(--remap-path-prefix="$HOME=/builder" --remap-path-prefix="$cargo_home=/cargo" \
    --remap-path-prefix="$rustup_home=/rustup" --remap-path-prefix="$PWD=/source")
export RUSTFLAGS="${RUSTFLAGS:-} ${remaps[*]}"

if [[ "$kind" == macos ]]; then
    say "build: the app binary"
    cargo build --release -p nocturne-app --features custom-protocol
    say "assemble: Nocturne.app and the .dmg"
    rm -rf "$pkg/macos"
    mkdir -p "$pkg/macos"
    scripts/macos-app.sh --manifest "$pkg/lemonade/$platform.manifest" target/release/nocturne-app "$pkg/lemonade/$platform" "$pkg/notices" "$pkg/macos"
    say "collect"
    rm -f "$pkg"/*.dmg
    cp "$pkg"/macos/*.dmg "$pkg/"
    (cd "$pkg" && "${sha256_tool[@]}" -- *.dmg > SHA256SUMS)
    ls -l "$pkg"/*.dmg
    cat "$pkg/SHA256SUMS"
    exit 0
fi

if [[ "$kind" == windows ]]; then
    rm -rf target/release/bundle/nsis target/release/bundle/msi "$pkg/windows"
    say "build: $bundles"
    build "$bundles"
    say "collect"
    mkdir -p "$pkg/windows"
    find target/release/bundle -type f \( -name '*.exe' -o -name '*.msi' \) -exec cp {} "$pkg/windows/" \;
    (cd "$pkg/windows" && sha256sum -- *.exe *.msi > SHA256SUMS) || { printf 'no installers were produced\n' >&2; exit 1; }
    cat "$pkg/windows/SHA256SUMS"
    exit 0
fi

rm -rf target/release/bundle/appimage target/release/bundle/deb target/release/bundle/rpm "$pkg/SOURCES.txt"
say "build: $bundles"
build "$bundles"

if [[ " ${targets[*]} " == *" appimage "* ]]; then
    # The AppImage tool rewrites the engine's executables and copies in system libraries.
    # Finish the AppDir by hand and pack it again: drop the modules the app does not use,
    # put the verified engine files back byte for byte, add the bundled libraries' licences,
    # and record their source packages beside the artefacts.
    appdir=$(find target/release/bundle/appimage -maxdepth 1 -name '*.AppDir' | head -1)
    [[ -n "$appdir" ]] || { printf 'the AppDir was not produced\n' >&2; exit 1; }
    plugin=${XDG_CACHE_HOME:-$HOME/.cache}/tauri/linuxdeploy-plugin-appimage.AppImage
    [[ -x "$plugin" ]] || { printf 'missing %s: the Tauri build should have fetched it\n' "$plugin" >&2; exit 1; }
    say "AppDir: prune, restore the engine, bundled library licences"
    python3 -I scripts/notices.py --prune-libs "$appdir/usr/lib"
    cp -a "$pkg/lemonade/$platform/." "$appdir/usr/lib/Nocturne/lemonade/"
    rm -rf "$pkg/notices-appimage"
    python3 -I scripts/notices.py --bundled-libs "$appdir/usr/lib" --out "$pkg/notices-appimage" --sources "$pkg/SOURCES.txt"
    cp "$pkg/notices-appimage/BUNDLED-LIBRARIES.txt" "$appdir/usr/lib/Nocturne/notices/"
    rm -f target/release/bundle/appimage/*.AppImage
    (cd target/release/bundle/appimage && ARCH=x86_64 LINUXDEPLOY_OUTPUT_VERSION="$version" "$plugin" --appdir "$(basename "$appdir")")
    packed=$(find target/release/bundle/appimage -maxdepth 1 -name '*.AppImage' | head -1)
    [[ -n "$packed" ]] || { printf 'the AppImage was not packed\n' >&2; exit 1; }
    mv "$packed" "target/release/bundle/appimage/Nocturne_${version}_amd64.AppImage"
fi

say "collect"
find "$pkg" -maxdepth 1 -type f \( -name '*.AppImage' -o -name '*.deb' -o -name '*.rpm' -o -name SHA256SUMS \) -delete
for t in "${targets[@]}"; do
    case "$t" in appimage) ext=AppImage ;; *) ext=$t ;; esac
    found=$(find "target/release/bundle/$t" -maxdepth 1 -type f -name "*.$ext" | head -1)
    [[ -n "$found" ]] || { printf 'no %s artefact was produced\n' "$t" >&2; exit 1; }
    cp "$found" "$pkg/"
done
(cd "$pkg" && sha256sum -- *.AppImage *.deb *.rpm $([[ -f SOURCES.txt ]] && printf SOURCES.txt) > SHA256SUMS)
ls -l "$pkg"/*.AppImage "$pkg"/*.deb "$pkg"/*.rpm
cat "$pkg/SHA256SUMS"
