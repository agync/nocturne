#!/usr/bin/env bash
# Vet and test the Most private helper (helpers/tailnet) twice: as a plain build, and with the
# release build tags. Offline: the tests use Tailscale's in-process control server only.
#
# GO=<path to go> picks the toolchain. Without the Go version pinned in the helper's go.mod
# (no Go, or another version) the tests are skipped with a warning, unless CI is set or
# NOCTURNE_REQUIRE_TAILNET_TESTS=1, which make that a failure.

set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
src=$root/helpers/tailnet
go_bin=${GO:-go}
jobs=${TAILNET_JOBS:-6}

want="go$(awk '$1 == "go" { print $2 }' "$src/go.mod")"
have=$(GOTOOLCHAIN=local "$go_bin" env GOVERSION 2> /dev/null || true)
if [ "${have%%[- ]*}" != "$want" ]; then
    msg="tailnet helper tests skipped: needs $want (found ${have:-no go}); set GO=<path to $want's go>"
    if [ -n "${CI:-}" ] || [ "${NOCTURNE_REQUIRE_TAILNET_TESTS:-}" = 1 ]; then
        echo "${msg/skipped/failed}" >&2
        exit 1
    fi
    echo "warning: $msg" >&2
    exit 0
fi

tags=$("$root/scripts/tailnet-tags.sh")
cd "$src"
export GOTOOLCHAIN=local GOFLAGS=-mod=readonly GOMAXPROCS=$jobs
for build in plain release; do
    flags=(-p "$jobs")
    [ "$build" = release ] && flags+=(-tags "$tags")
    echo "== tailnet/test: $build build"
    "$go_bin" vet "${flags[@]}" ./...
    "$go_bin" test "${flags[@]}" -count=1 ./...
done
