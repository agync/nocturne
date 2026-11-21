#!/usr/bin/env bash
# Refuse a phone app release that could reach a computer on loopback (`dev-loopback`).
#
# Usage: phone/check-release.sh --features          before building: the release feature set
#        phone/check-release.sh <apk-or-aab>...     after building: the library in each package
#
# Fails when the feature is on, when no package is named, when a package holds no phone library,
# or when a library holds the marker the feature adds.
set -euo pipefail

marker=nocturne-dev-loopback-marker
lib=libnocturne_phone_lib.so

if [ "${1:-}" = --features ]; then
    tree=$(cargo tree -p nocturne-phone --target aarch64-linux-android -e features -i nocturne-channel)
    if grep -q dev-loopback <<< "$tree"; then
        echo "the release would build nocturne-channel with dev-loopback" >&2
        exit 1
    fi
    echo "ok    no dev-loopback in the release features"
    exit 0
fi

if [ "$#" -eq 0 ]; then
    echo "no package to check" >&2
    exit 1
fi
for package in "$@"; do
    libs=$(unzip -Z1 "$package" 2> /dev/null | grep -E "(^|/)lib/[^/]+/$lib$" || true)
    if [ -z "$libs" ]; then
        echo "no $lib in $package" >&2
        exit 1
    fi
    while IFS= read -r entry; do
        # grep -c reads to the end: with -q, unzip's broken pipe would fail the pipeline and
        # read as clean.
        found=$(unzip -p "$package" "$entry" | grep -ac "$marker" || true)
        if [ "$found" != 0 ]; then
            echo "a dev-loopback build is in $package ($entry)" >&2
            exit 1
        fi
        echo "ok    $package: $entry"
    done <<< "$libs"
done
