#!/usr/bin/env bash
# Assemble Nocturne.app and a .dmg from a release binary, on a Mac, with the tools macOS ships
# (PlistBuddy, plutil, codesign, hdiutil). Used by scripts/package.sh --config macos, so the
# Mac needs no Node: the Tauri CLI is not involved.
#
# Usage: scripts/macos-app.sh [--sign IDENTITY] BINARY LEMONADE_DIR NOTICES_DIR OUT_DIR
#
# Writes OUT_DIR/Nocturne.app and OUT_DIR/Nocturne_<version>_aarch64.dmg. The signature is
# ad hoc ("-") unless --sign names an identity: enough to run on the Mac that built it, not to
# hand to anyone else (that needs a Developer ID and notarisation; see docs/contribution/
# building.md). The engine's files are copied byte for byte and are not touched by signing.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

identity=-
if [[ "${1:-}" == --sign ]]; then
    identity="${2:?--sign needs an identity}"
    shift 2
fi
[[ $# -eq 4 ]] || { sed -n '2,12p' "$0" >&2; exit 2; }
binary=$1 lemonade=$2 notices=$3 out=$4
[[ "$(uname -s)" == Darwin ]] || { printf 'the app is assembled on a Mac\n' >&2; exit 2; }
for need in "$binary" "$lemonade/lemond" "$notices/THIRD-PARTY-NOTICES.txt" src-tauri/icons/icon.icns; do
    [[ -e "$need" ]] || { printf 'missing: %s\n' "$need" >&2; exit 1; }
done

conf=src-tauri/bundle/macos.json
plist_value() { plutil -extract "$2" raw -o - "$1"; }
version=$(python3 -I -c 'import sys; sys.path.insert(0, "scripts"); import bundle_config; print(bundle_config.workspace_version())')
identifier=$(plist_value src-tauri/tauri.conf.json identifier)
name=$(plist_value src-tauri/tauri.conf.json productName)
exe=$(plist_value "$conf" mainBinaryName)
minimum=$(plist_value "$conf" bundle.macOS.minimumSystemVersion)
entitlements=src-tauri/$(plist_value "$conf" bundle.macOS.entitlements)

app="$out/$name.app"
rm -rf "$app" "$out"/*.dmg
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$binary" "$app/Contents/MacOS/$exe"
chmod 755 "$app/Contents/MacOS/$exe"
cp src-tauri/icons/icon.icns "$app/Contents/Resources/icon.icns"
cp -R "$lemonade" "$app/Contents/Resources/lemonade"
cp -R "$notices" "$app/Contents/Resources/notices"
cp LICENSE "$app/Contents/Resources/LICENSE"

info="$app/Contents/Info.plist"
buddy=/usr/libexec/PlistBuddy
for entry in \
    "CFBundleDevelopmentRegion string en" \
    "CFBundleExecutable string $exe" \
    "CFBundleIconFile string icon" \
    "CFBundleIdentifier string $identifier" \
    "CFBundleInfoDictionaryVersion string 6.0" \
    "CFBundleName string $name" \
    "CFBundleDisplayName string $name" \
    "CFBundlePackageType string APPL" \
    "CFBundleShortVersionString string $version" \
    "CFBundleVersion string $version" \
    "LSApplicationCategoryType string public.app-category.productivity" \
    "LSMinimumSystemVersion string $minimum" \
    "NSHighResolutionCapable bool true"; do
    read -r key type value <<< "$entry"
    "$buddy" -c "Add :$key $type $value" "$info"
done
# The microphone sentence lives in src-tauri/Info.plist, which the Tauri CLI merges the same way.
"$buddy" -c "Merge src-tauri/Info.plist" "$info"
plutil -lint "$info" > /dev/null

codesign --force --deep --sign "$identity" --entitlements "$entitlements" "$app"
codesign --verify --deep --strict "$app"

dmg="$out/${name}_${version}_aarch64.dmg"
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
cp -R "$app" "$stage/"
ln -s /Applications "$stage/Applications"
hdiutil create -quiet -volname "$name" -srcfolder "$stage" -ov -format UDZO "$dmg"
printf '%s\n%s\n' "$app" "$dmg"
