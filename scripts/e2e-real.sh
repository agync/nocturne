#!/usr/bin/env bash
# Run the real end-to-end test, then run it again with the network cut and traced.
#
# Usage: scripts/e2e-real.sh [run|offline|all]     (default: all)
#   run      the test with real models. The first run downloads the small mind (about 4.8 GB)
#            through the app's own setup into NOCTURNE_E2E_DIR; later runs reuse it.
#   offline  the same test in a private network namespace (loopback only) under strace, on what
#            `run` already downloaded, judged by scripts/netlog.py: capture, reading, answers, the
#            tidy and forgetting must put nothing on the network.
#   all      run, then offline.
#
# NOCTURNE_E2E_DIR is the scratch folder (default target/e2e-real). It is the only place models
# are downloaded to, and it is never the app's own data folder. Results are written to
# $NOCTURNE_E2E_DIR/results/ (results.json and summary.md; results-offline.* for the second run).
set -uo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
repo=$PWD
mode=${1:-all}
dir=${NOCTURNE_E2E_DIR:-$repo/target/e2e-real}
mkdir -p "$dir"
dir=$(realpath -- "$dir")
export NOCTURNE_E2E_DIR=$dir

scripts/fetch-lemonade.sh || exit 2

run_online() {
    printf '== e2e-real: real models, scratch folder %s\n' "$dir"
    cargo test -p nocturne-app --test e2e_real -- --ignored --nocapture --test-threads=1
}

test_binary() {
    cargo test -p nocturne-app --test e2e_real --no-run --message-format=json 2> /dev/null |
        python3 -I -c '
import json, sys
for line in sys.stdin:
    try:
        m = json.loads(line)
    except ValueError:
        continue
    if m.get("reason") == "compiler-artifact" and m.get("executable") and m["target"]["name"] == "e2e_real":
        print(m["executable"])
'
}

run_offline() {
    for tool in unshare strace ip python3; do
        command -v "$tool" > /dev/null || { printf 'missing tool: %s\n' "$tool" >&2; return 2; }
    done
    unshare -rn true 2> /dev/null || { printf 'a user and network namespace is not available here\n' >&2; return 2; }
    local bin trace code
    bin=$(test_binary)
    [[ -x "$bin" ]] || { printf 'could not find the test binary\n' >&2; return 2; }
    trace=$dir/trace-offline.log
    rm -f "$trace"
    printf '== e2e-real: network cut, loopback only, traced\n'
    NOCTURNE_E2E_TAG=-offline unshare -rn bash -c '
        ip link set lo up || exit 2
        exec strace -f --seccomp-bpf -qq -e trace=%network,io_uring_setup -s 256 -o "$0" "$@"
    ' "$trace" "$bin" --ignored --nocapture --test-threads=1
    code=$?
    printf '== outbound report\n'
    python3 -I scripts/netlog.py --scratch "$dir" "$trace"
    local judged=$?
    ((code == 0 && judged == 0))
}

case "$mode" in
    run) run_online ;;
    offline) run_offline ;;
    all)
        # The second run is a measurement of its own: it happens whatever the first one found.
        run_online
        online=$?
        run_offline
        offline=$?
        exit $((online != 0 || offline != 0))
        ;;
    *) printf 'unknown mode: %s\n' "$mode" >&2; exit 2 ;;
esac
