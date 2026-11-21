#!/usr/bin/env bash
# Run the real end-to-end test, then run it again with the network cut and traced.
#
# Usage: scripts/e2e-real.sh [run|offline|all|online|models|relay|help|decisions]   (default: all)
#   run      the test with real models. The first run downloads the small mind (about 4.8 GB)
#            through the app's own setup into NOCTURNE_E2E_DIR; later runs reuse it.
#   offline  the same test in a private network namespace (loopback only) under strace, on what
#            `run` already downloaded, judged by scripts/netlog.py: capture, reading, answers, the
#            tidy and forgetting must put nothing on the network.
#   all      run, then offline.
#   online   the test with Online answers on against the stand-in provider (`make e2e-online`,
#            a build with the online-stand-in feature). Unless NOCTURNE_E2E_NO_RELAY=1 the stand-in
#            relays to a second pinned lemond this script starts on loopback under its own key,
#            serving NOCTURNE_E2E_RELAY_MODEL (default Qwen3.5-9B-GGUF, the Standard mind) from
#            $NOCTURNE_E2E_DIR/relay, which is the only place it keeps files. A model it lacks is
#            pulled through its catalogue, the only network use of the mode. That engine is not a
#            child of the test, so the memory the test reports excludes it. Stopped by its pid at
#            the end. NOCTURNE_E2E_TIER, NOCTURNE_E2E_TAG and the other variables work as in `run`.
#            The relay answers without a thinking phase (--reasoning-budget 0, NOCTURNE_E2E_RELAY_ARGS
#            replaces the engine flags), as a plain hosted answerer would for a request that does
#            not ask for thinking.
#   relay    only gets the relay engine ready: pulls its model if it is missing, then stops it.
#   models   the test with a model chosen for each job, NOCTURNE_E2E_MODELS="mind=<id>,speech=<id>,
#            voice=<id>,recall=<id>" (any of the four), through the app's own change command with
#            the download agreed (`make e2e-models`).
#   help     the help check (docs/design/in-app-help.md) on the size NOCTURNE_E2E_TIER sets up:
#            the 29 questions, NOCTURNE_E2E_HELP times (default 3), with help written here
#            (`make e2e-help`). Results in $NOCTURNE_E2E_DIR/results/help.json.
#   decisions the decision check (docs/design/evaluation.md) on the size NOCTURNE_E2E_TIER sets up:
#            the rules, the mind and the decision model this computer's hardware picks (or
#            NOCTURNE_E2E_DECISION_MODEL), NOCTURNE_E2E_DECISIONS times (default 1). A decision model
#            missing from the scratch folder is downloaded into it, the mode's only network use
#            beyond setup (`make e2e-decisions`). Results in $NOCTURNE_E2E_DIR/results/decisions.json.
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

relay_pid=""
stop_relay() {
    if [[ -n "$relay_pid" ]]; then
        kill "$relay_pid" 2> /dev/null
        for ((i = 0; i < 100; i++)); do
            kill -0 "$relay_pid" 2> /dev/null || break
            sleep 0.1
        done
        kill -0 "$relay_pid" 2> /dev/null && kill -KILL "$relay_pid" 2> /dev/null
        relay_pid=""
    fi
}
trap stop_relay EXIT

free_port() {
    python3 -I -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])'
}

# The relay's config: the settings the app gives its own engine, in a folder of its own.
relay_config() {
    python3 -I - "$1" "$2" "$3" "$4" << 'PY'
import json, sys
root, offline, ws, args = sys.argv[1], sys.argv[2] == "1", int(sys.argv[3]), sys.argv[4]
config = {
    "host": "127.0.0.1", "broadcast": False, "models_dir": f"{root}/models", "offline": offline,
    "no_fetch_executables": True, "auto_check_model_updates": False, "auto_update_models": False,
    "inhibit_suspend": False, "auto_evict": True, "max_loaded_models": 1, "ctx_size": 8192,
    "log_level": "info", "websocket_port": ws, "cloud_providers": [],
    "telemetry": {"enabled": False}, "llamacpp": {"args": args, "backend": "vulkan"},
}
json.dump(config, open(f"{root}/config/config.json", "w"), indent=2)
PY
}

# Starts a pinned lemond on loopback under `key` and waits until it is live. Sets relay_pid.
relay_launch() {
    local root=$1 port=$2 key=$3
    mkdir -p "$root/run" "$root/logs"
    chmod 700 "$root/run"
    setsid env -i HOME="$root" PATH="$PATH" XDG_RUNTIME_DIR="$root/run" LANG=C.UTF-8 \
        LEMONADE_API_KEY="$key" LEMONADE_ADMIN_API_KEY="$key-admin" \
        "$bundle/lemond" "$root/cache" "$root/config" --port "$port" --host 127.0.0.1 --no-broadcast \
        > "$root/logs/lemond.log" 2>&1 < /dev/null &
    relay_pid=$!
    for ((i = 0; i < 300; i++)); do
        curl -s --noproxy '*' -o /dev/null "http://127.0.0.1:$port/live" && return 0
        kill -0 "$relay_pid" 2> /dev/null || { printf 'the relay engine exited early (see %s)\n' "$root/logs/lemond.log" >&2; return 1; }
        sleep 0.2
    done
    printf 'the relay engine never came up\n' >&2
    return 1
}

start_relay() {
    local model=${NOCTURNE_E2E_RELAY_MODEL:-Qwen3.5-9B-GGUF}
    local root=$dir/relay port ws key args=${NOCTURNE_E2E_RELAY_ARGS:---cache-ram 256 --ctx-checkpoints 4 --reasoning-budget 0}
    bundle=${NOCTURNE_LEMOND:-$repo/vendor/lemonade/linux-x86_64}
    [[ -x "$bundle/lemond" ]] || { printf 'no pinned lemond at %s\n' "$bundle" >&2; return 2; }
    mkdir -p "$root/cache/bin" "$root/config" "$root/models"
    if [[ ! -d "$root/cache/bin/llamacpp" ]]; then
        [[ -d "$dir/data/engine/cache/bin/llamacpp" ]] || { printf 'no llama.cpp backend in %s to reuse\n' "$dir/data/engine/cache/bin" >&2; return 2; }
        cp -a "$dir/data/engine/cache/bin/llamacpp" "$root/cache/bin/llamacpp"
    fi
    port=$(free_port)
    ws=$(free_port)
    key=relay-$(python3 -I -c 'import secrets; print(secrets.token_hex(16))')
    local present
    present=$(find "$root/models" -maxdepth 1 -iname "*${model%-GGUF}*" 2> /dev/null | head -1)
    if [[ -z "$present" ]]; then
        printf '== relay: pulling %s into %s\n' "$model" "$root/models"
        relay_config "$root" 0 "$ws" "$args"
        relay_launch "$root" "$port" "$key" || return 1
        curl -s --noproxy '*' -H "Authorization: Bearer $key" -H 'Content-Type: application/json' \
            -d "{\"model_name\": \"$model\", \"stream\": true, \"subscribe\": false}" "http://127.0.0.1:$port/v1/pull" > "$root/logs/pull-start.json"
        local last=-1 same=0
        while :; do
            local job
            job=$(curl -s --noproxy '*' -H "Authorization: Bearer $key" "http://127.0.0.1:$port/v1/downloads")
            read -r running done_n bytes err < <(python3 -I -c '
import json, sys
try:
    jobs = json.loads(sys.stdin.read())
except ValueError:
    jobs = []
jobs = jobs if isinstance(jobs, list) else jobs.get("downloads", [])
running = sum(1 for j in jobs if j.get("running"))
done_n = sum(1 for j in jobs if j.get("complete"))
bytes_ = max([max(j.get("cumulative_bytes_downloaded", 0), j.get("overall_bytes_downloaded", 0), j.get("bytes_downloaded", 0)) for j in jobs] or [0])
err = next((j.get("error") for j in jobs if j.get("error")), "") or "-"
print(running, done_n, bytes_, err.replace(" ", "_"))
' <<< "$job")
            printf 'relay pull: running %s complete %s bytes %s error %s\n' "$running" "$done_n" "$bytes" "$err"
            [[ "$err" != "-" ]] && { printf 'the relay pull failed\n' >&2; return 1; }
            [[ "$running" == 0 && "$done_n" -ge 1 ]] && break
            if [[ "$bytes" == "$last" ]]; then same=$((same + 1)); else same=0; last=$bytes; fi
            ((same > 90)) && { printf 'the relay pull stalled\n' >&2; return 1; }
            sleep 20
        done
        stop_relay
    fi
    relay_config "$root" 1 "$ws" "$args"
    relay_launch "$root" "$port" "$key" || return 1
    export NOCTURNE_E2E_ONLINE_RELAY="http://127.0.0.1:$port/v1" NOCTURNE_E2E_ONLINE_RELAY_KEY="$key" NOCTURNE_E2E_ONLINE_MODEL="$model"
    printf '== relay: %s on 127.0.0.1:%s, pid %s\n' "$model" "$port" "$relay_pid"
}

run_stand_in() {
    printf '== e2e-real: Online answers against the stand-in, scratch folder %s\n' "$dir"
    export NOCTURNE_E2E_ONLINE=stand-in
    if [[ "${NOCTURNE_E2E_NO_RELAY:-0}" != 1 ]]; then
        start_relay || { stop_relay; return 2; }
    fi
    cargo test -p nocturne-app --features online-stand-in --test e2e_real -- --ignored --nocapture --test-threads=1
    local code=$?
    stop_relay
    return $code
}

run_models() {
    [[ -n "${NOCTURNE_E2E_MODELS:-}" ]] || { printf 'set NOCTURNE_E2E_MODELS="mind=<id>,..."\n' >&2; return 2; }
    printf '== e2e-real: chosen models %s, scratch folder %s\n' "$NOCTURNE_E2E_MODELS" "$dir"
    cargo test -p nocturne-app --test e2e_real -- --ignored --nocapture --test-threads=1
}

run_help() {
    printf '== e2e-real: the help check, tier %s, scratch folder %s\n' "${NOCTURNE_E2E_TIER:-small}" "$dir"
    NOCTURNE_E2E_HELP=${NOCTURNE_E2E_HELP:-3} \
        cargo test -p nocturne-app --test e2e_real -- --ignored --nocapture --test-threads=1
}

run_decisions() {
    printf '== e2e-real: the decision check, tier %s, scratch folder %s\n' "${NOCTURNE_E2E_TIER:-small}" "$dir"
    NOCTURNE_E2E_DECISIONS=${NOCTURNE_E2E_DECISIONS:-1} \
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
    online) run_stand_in ;;
    models) run_models ;;
    help) run_help ;;
    decisions) run_decisions ;;
    relay) start_relay && stop_relay ;;
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
