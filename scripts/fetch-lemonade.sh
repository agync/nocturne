#!/usr/bin/env bash
# Download the pinned Lemonade embeddable bundle into vendor/lemonade/<platform>, verifying sha256.
# Usage: scripts/fetch-lemonade.sh [platform]   (default: this machine)
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
source scripts/sha256.sh

platform="${1:-}"
if [[ -z "$platform" ]]; then
    case "$(uname -s)-$(uname -m)" in
        Linux-x86_64) platform=linux-x86_64 ;;
        Linux-aarch64) platform=linux-aarch64 ;;
        Darwin-arm64) platform=macos-aarch64 ;;
        MINGW*-x86_64 | MSYS*-x86_64) platform=windows-x86_64 ;;
        *) printf 'unsupported platform: %s\n' "$(uname -s)-$(uname -m)" >&2; exit 2 ;;
    esac
fi

field() { sed -n "s/^${platform} = .*${1} = \"\([^\"]*\)\".*/\1/p" lemonade.lock; }
base_url=$(sed -n 's/^base_url = "\(.*\)"/\1/p' lemonade.lock)
file=$(field file)
sum=$(field sha256)
[[ -n "$file" && -n "$sum" ]] || { printf 'no asset for %s in lemonade.lock\n' "$platform" >&2; exit 2; }

dest="vendor/lemonade/$platform"
stamp="$dest/.sha256"
if [[ -f "$stamp" && "$(cat "$stamp")" == "$sum" ]]; then
    printf 'lemonade %s already present\n' "$platform"
    exit 0
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
curl --fail --location --silent --show-error -o "$tmp/$file" "$base_url/$file"
[[ "$(sha256_of "$tmp/$file")" == "$sum" ]] || { printf 'sha256 mismatch for %s\n' "$file" >&2; exit 1; }

rm -rf "$dest"
mkdir -p "$dest"
case "$file" in
    *.zip) unzip -q "$tmp/$file" -d "$tmp/x" ;;
    *) mkdir -p "$tmp/x" && tar -xzf "$tmp/$file" -C "$tmp/x" ;;
esac
# The archive holds one top-level directory; flatten it.
inner=$(find "$tmp/x" -mindepth 1 -maxdepth 1 -type d | head -1)
cp -a "${inner:-$tmp/x}/." "$dest/"
printf '%s' "$sum" > "$stamp"
printf 'lemonade %s -> %s\n' "$platform" "$dest"
