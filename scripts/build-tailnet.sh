#!/usr/bin/env bash
# Build the Most private helper (helpers/tailnet) for every platform Nocturne ships, reproducibly:
# the pinned Go toolchain, no cgo, no paths, no VCS stamp, no build id. Prints each sha256.
#
# Usage: scripts/build-tailnet.sh [OUT_DIR]   (default target/tailnet)
# GO=<path to go> picks the toolchain when `go` on PATH is another version.

set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
src=$root/helpers/tailnet
out=${1:-$root/target/tailnet}
go_bin=${GO:-go}
# shellcheck source=scripts/sha256.sh
. "$root/scripts/sha256.sh"

want="go$(awk '$1 == "go" { print $2 }' "$src/go.mod")"
tailscale=$(awk '$1 == "require" && $2 == "tailscale.com" { print substr($3, 2) }' "$src/go.mod")
have=$(GOTOOLCHAIN=local "$go_bin" env GOVERSION)
if [ "$have" != "$want" ]; then
    echo "build-tailnet: need $want, found $have (set GO to the pinned toolchain)" >&2
    exit 1
fi

tags=$("$root/scripts/tailnet-tags.sh")

ldflags="-buildid= -s -w"
ldflags+=" -X tailscale.com/version.longStamp=$tailscale -X tailscale.com/version.shortStamp=$tailscale"

mkdir -p "$out"
out=$(cd "$out" && pwd)
for platform in linux/amd64/linux-x86_64 linux/arm64/linux-aarch64 darwin/arm64/macos-aarch64 \
    windows/amd64/windows-x86_64; do
    IFS=/ read -r goos goarch name <<< "$platform"
    file=$out/nocturne-tailnet-$name
    [ "$goos" = windows ] && file+=.exe
    (
        cd "$src"
        env GOTOOLCHAIN=local GOFLAGS=-mod=readonly CGO_ENABLED=0 GOOS="$goos" GOARCH="$goarch" \
            GOAMD64=v1 GOARM64=v8.0 GOEXPERIMENT= GOFIPS140=off \
            "$go_bin" build -trimpath -buildvcs=false -tags "$tags" -ldflags "$ldflags" -o "$file" .
    )
    echo "$(sha256_of "$file")  $(basename "$file")"
done
