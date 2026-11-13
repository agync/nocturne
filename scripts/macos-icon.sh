#!/usr/bin/env bash
# Build src-tauri/icons/icon.icns from icon.png with the tools macOS ships (sips, iconutil).
# Usage: scripts/macos-icon.sh      (run on a Mac; commit the result)
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

src=src-tauri/icons/icon.png
out=src-tauri/icons/icon.icns
iconset=$(mktemp -d)/Nocturne.iconset
trap 'rm -rf "$(dirname "$iconset")"' EXIT
mkdir -p "$iconset"
# The source is 512 pixels, so the largest slot is 512 and nothing is scaled up.
for spec in 16:icon_16x16 32:icon_16x16@2x 32:icon_32x32 64:icon_32x32@2x 128:icon_128x128 \
    256:icon_128x128@2x 256:icon_256x256 512:icon_256x256@2x 512:icon_512x512; do
    sips -z "${spec%%:*}" "${spec%%:*}" "$src" --out "$iconset/${spec#*:}.png" > /dev/null
done
iconutil --convert icns --output "$out" "$iconset"
printf 'wrote %s\n' "$out"
