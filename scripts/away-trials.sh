#!/usr/bin/env bash
# Away from home, the owner-approved real-network trials (plan 12, D10). Unlike
# scripts/away-integration.sh these reach the internet: the computer's side downloads the pinned
# cloudflared from GitHub and opens a Cloudflare quick tunnel, or starts the tailnet helper, which
# signs in to the owner's Tailscale account. Ask the owner before running any of them.
#
# Usage: TRIALS_DIR=<folder> scripts/away-trials.sh [session|edge|idle|phone]
#   session  pair, keep, ask, page, original, take back; the phone gets the built app byte for
#            byte; private stays home; removal mid-answer and mid-upload (TRIAL_CONNECTION=simple
#            or mostPrivate; Most private needs AWAY_REAL_TAILNET, a tailnet helper checked
#            against helpers.lock, and writes the sign-in link to TRIALS_DIR/TAILNET-LOGIN.txt)
#   edge     Simple: the metrics line, a hostile metrics-port holder, a bounded burst, old names
#   idle     Simple: memory off, on and idle, an idle phone for TRIAL_IDLE_MINUTES (default 60),
#            a frozen and a killed helper, re-linking, a 50 MB upload
#   phone    a trial pairing for a real phone, with demo data (see the phone checklist)
#
# Data lives in throwaway folders under TRIALS_DIR, never the person's own. Results are JSON lines
# in TRIALS_DIR. Heavy: run it under the machine's resource cap (docs/contribution/building.md).
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
: "${TRIALS_DIR:?TRIALS_DIR names the folder for results and throwaway data}"
what=${1:-session}
case "$what" in session | edge | idle | phone) ;; *) printf 'usage: %s [session|edge|idle|phone]\n' "$0" >&2; exit 2 ;; esac

cargo test -p nocturne-app --features fake-helpers --test away_driver --no-run
npm --prefix ui run build:phone
mkdir -p "$TRIALS_DIR/tmp-$what"
export TMPDIR="$TRIALS_DIR/tmp-$what"
if [[ "$what" == phone ]]; then
    exec node ui/e2e-trials/pair-phone.mjs
fi
cd ui && exec npx playwright test --config playwright.trials.config.ts "$what.spec.ts"
