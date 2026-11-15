#!/usr/bin/env bash
# Away from home, integration runs that need the computer's side built from Rust.
#
# Usage: scripts/away-integration.sh [interop|e2e|real|netcheck|all]
#   interop   the Rust test phone and the TypeScript phone client against each other's real
#             implementation (ui/interop, vitest)
#   e2e       the built phone app in Chromium at 390 x 844 against the real app through the
#             stand-in helper (ui/e2e-away, Playwright): the whole session with a scanned and a
#             typed code, and the queue after the computer goes away
#   real      Linux only: the real desktop app under a virtual display with the stand-in helper:
#             turn on, pair, upload 50 MB, turn off, quit; nothing left; memory and timer budgets
#   netcheck  `make netcheck` on the release binary, then `make netcheck-remote` on a release
#             binary built with the stand-in helpers (needs vendor/lemonade/linux-x86_64)
#   all       interop and e2e (the default)
#
# Heavy: run it under the machine's resource cap (docs/contribution/building.md). Nothing here
# reaches a network beyond loopback, starts a real helper, or touches the person's own data.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
repo=$PWD

what=${1:-all}
case "$what" in interop | e2e | real | netcheck | all) ;; *) printf 'usage: %s [interop|e2e|real|netcheck|all]\n' "$0" >&2; exit 2 ;; esac

status=0

# shellcheck source=scripts/away-test-programs.sh
source "$(dirname "${BASH_SOURCE[0]}")/away-test-programs.sh"

if [[ "$what" == interop || "$what" == e2e || "$what" == all ]]; then
    cargo build -p nocturne-helpers --features fake
    build_tests away_driver away_phone
    npm --prefix ui run build:phone
    if [[ "$what" == interop || "$what" == all ]]; then
        (cd ui && npx vitest run --config vitest.interop.config.ts) || status=1
    fi
    if [[ "$what" == e2e || "$what" == all ]]; then
        (cd ui && npx playwright test --config playwright.away.config.ts) || status=1
    fi
fi

if [[ "$what" == real ]]; then
    cargo build -p nocturne-helpers --features fake
    npm --prefix ui run build
    cargo build -p nocturne-app --features custom-protocol,fake-mind,fake-helpers
    (cd ui && AWAY_REAL_APP="$repo/target/debug/nocturne-app" \
        npx playwright test --config playwright.away.config.ts -g "the real app") || status=1
fi

if [[ "$what" == netcheck ]]; then
    [[ -d vendor/lemonade/linux-x86_64 ]] || { printf 'no vendor/lemonade/linux-x86_64 (make lemonade/fetch)\n' >&2; exit 2; }
    npm --prefix ui run build
    cargo build --release -p nocturne-app --features custom-protocol
    NET_CHECK_BINARY="$repo/target/release/nocturne-app" make netcheck || status=1

    cargo build -p nocturne-helpers --features fake
    build_tests away_driver
    cargo build --release -p nocturne-app --features custom-protocol,fake-helpers
    prepared=$(mktemp -d)
    (cd ui && AWAY_NETCHECK_STORE="$prepared" npx vitest run --config vitest.interop.config.ts store.test.ts)
    NET_CHECK_BINARY="$repo/target/release/nocturne-app" NET_CHECK_DIR="$prepared" \
        NET_CHECK_FAKES="$repo/target/debug" make netcheck-remote || status=1
    rm -rf "$prepared"
fi
exit "$status"
