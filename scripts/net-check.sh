#!/usr/bin/env bash
# Prove Nocturne talks to nothing outside this computer.
#
# Usage: scripts/net-check.sh [launch|prepared|remote|online|attach] [binary|appimage|deb|rpm]
#   launch    fresh data folder, no setup: idle NET_SECONDS (default 120), then a second launch
#             with a text file. Needs no input. This is what `make netcheck` runs on the binary.
#   prepared  NET_CHECK_DIR names a data folder where setup finished (never the person's own
#             app data). It runs on a scratch copy of its store/ with engine/models linked in
#             place (not copied), keeps a text note and an image so the mind loads and reads,
#             and idles NET_SECONDS (default 300).
#   remote    Away from home on, through the stand-in helpers (`make netcheck-remote`). Needs a binary
#             built with the helpers crate's `fake` feature (never packaged), NET_CHECK_DIR as for
#             prepared but with Away from home turned on in its store, and the stand-in helpers
#             built beside it (NET_CHECK_FAKES, default target/debug: fake-cloudflared,
#             fake-tailnet; NET_CHECK_CONNECTION=simple|private picks which one
#             the app is expected to start, default simple). The script installs the stand-ins as
#             the pinned helpers under the scratch data folder with a lock of their own
#             (NOCTURNE_FAKE_HELPERS_LOCK) and asks them to try 198.51.100.1:7844 and to bind UDP on
#             every interface. NET_CHECK_PHONE may name a command that plays a scripted phone against
#             the app's data folder (passed as its argument). The judge allows non-loopback attempts
#             and the any-interface UDP bind for helper programs only; the app and lemond never.
#   online    Online answers on, for an address no namespace can reach (`NET_CHECK_PROVIDER`, default
#             198.51.100.7:8443, TEST-NET-2, an IP literal so nothing is looked up), with the key in the
#             owner-only file. Runs a scripted driver (src-tauri/tests/netcheck_script.rs, a test build
#             of the app over the real engine, never packaged) that asks NET_CHECK_ASKS questions
#             (default 3), idles NET_SECONDS (default 600), turns Online answers off and asks again,
#             writing a mark for each step. The log is traced with timestamps and netlog.py judges it in
#             online mode: the engine alone may connect to the one address, once per ask, never while
#             idle and never after the off mark; the app never reaches outside. The variant is ignored.
#   attach    wraps a manual session on a real display: ask, listen, print; Ctrl+C ends it and
#             the report is printed. Pass the display by socket path, because abstract sockets
#             (the usual X11 form) do not cross the network namespace:
#               NET_CHECK_WAYLAND=/run/user/1000/wayland-0   (an absolute path)
#               NET_CHECK_PULSE=unix:/run/user/1000/pulse/native
#               NET_CHECK_DISPLAY=:0                          (an X server with a path socket)
#             The session bus is private and the data folder is scratch.
#
# How: the app runs in a private user and network namespace (`unshare -rn`) where only loopback
# exists, under `strace -f -e trace=%network`. scripts/netlog.py fails on any non-loopback
# destination, any port 53, and any connection to a name-service socket, and the run must show
# the app really worked (interface ready, and a kept file for the launch modes). In the modes
# with Away from home off it also fails on any listen by the app, any exec of a helper and any
# file under <data>/remote/helpers. Nothing is installed; processes are stopped by the pid found
# for the app.
set -uo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
repo=$PWD
pkg=$repo/target/package

# Stage 2: inside the namespace, bring loopback up and start the display and session bus.
if [[ "${1:-}" == --in-ns ]]; then
    shift
    ip link set lo up || { printf 'could not bring loopback up\n' >&2; exit 2; }
    if [[ "$1" == attach ]]; then
        exec dbus-run-session -- "$repo/scripts/net-check.sh" --inner "$@"
    fi
    exec xvfb-run -a -s "-screen 0 1280x800x24" dbus-run-session -- "$repo/scripts/net-check.sh" --inner "$@"
fi

# Stage 3: inside the namespace, display and bus. Runs the app under strace and judges.
if [[ "${1:-}" == --inner ]]; then
    shift
    mode=$1 launch=$2 procexe=$3 bundle=$4 scratch=$5 label=$6
    seconds=${NET_SECONDS:-120}
    [[ "$mode" == prepared ]] && seconds=${NET_SECONDS:-300}
    [[ "$mode" == remote ]] && seconds=${NET_SECONDS:-60}
    [[ "$mode" == online ]] && seconds=${NET_SECONDS:-600}
    provider=${NET_CHECK_PROVIDER:-198.51.100.7:8443}
    # The scratch environment was exported before the namespace and the session bus started.
    mkdir -p "$scratch/stderr"
    result=$scratch/result.txt
    fail=0
    bad() { printf 'FAIL  %s: %s\n' "$label" "$*" >> "$result"; fail=1; }
    good() { printf 'ok    %s: %s\n' "$label" "$*" >> "$result"; }
    traced=%network,io_uring_setup,execve,clone,clone3,fork,vfork
    stamps=()
    [[ "$mode" == online ]] && stamps=(-ttt)
    trace() { strace -f -qq "${stamps[@]}" -e trace="$traced" -s 256 -o "$1" "${@:2}"; }
    helpers_dir=$scratch/data/remote/helpers
    on_or_off=off
    [[ "$mode" == remote ]] && on_or_off=remote
    [[ "$mode" == online ]] && on_or_off=online

    if [[ "$mode" == prepared || "$mode" == remote ]]; then
        src=${NET_CHECK_DIR:?this mode needs NET_CHECK_DIR}
        [[ -d "$src/store" ]] || { printf 'NET_CHECK_DIR has no store/\n' >&2; exit 2; }
        cp -a "$src/store" "$scratch/data/store"
        mkdir -p "$scratch/data/engine"
        [[ -d "$src/engine/models" ]] && ln -s "$src/engine/models" "$scratch/data/engine/models"
    fi

    # Away from home on: the stand-in helpers stand in for the pinned ones, each beside its own
    # control file, under a lock that pins their checksums.
    if [[ "$mode" == remote ]]; then
        fakes=${NET_CHECK_FAKES:-$repo/target/debug}
        connection=${NET_CHECK_CONNECTION:-simple}
        case "$connection" in
            simple) kinds="cloudflared:fake-cloudflared:cloudflared" ;;
            private) kinds="tailnet:fake-tailnet:nocturne-tailnet" ;;
            *) printf 'NET_CHECK_CONNECTION is simple or private\n' >&2; exit 2 ;;
        esac
        expected=
        for pair in $kinds; do
            IFS=: read -r kind fake name <<< "$pair"
            [[ -x "$fakes/$fake" ]] || { printf 'missing %s/%s (cargo build -p nocturne-helpers --features fake)\n' "$fakes" "$fake" >&2; exit 2; }
            dest=$helpers_dir/$kind/fake
            mkdir -p "$dest" && chmod 700 "$helpers_dir" "$helpers_dir/$kind" "$dest"
            cp "$fakes/$fake" "$dest/$name"
            # The tailnet helper serves only inside its own process, so its stand-in forwards
            # nothing here: a listen by it fails the check.
            if [[ "$kind" == tailnet ]]; then
                printf '{"edge_attempt": true, "udp_any": true, "serve": false}\n' > "$dest/fake-control.json"
            else
                printf '{"edge_attempt": true, "udp_any": true}\n' > "$dest/fake-control.json"
            fi
            expected+=" $dest"
        done
        python3 -I - "$scratch/fake.lock" "$helpers_dir" << 'PY'
import hashlib, sys
out, helpers = sys.argv[1], sys.argv[2]
def sha(path):
    return hashlib.sha256(open(path, "rb").read()).hexdigest()
text = ""
for kind, name in (("cloudflared", "cloudflared"), ("tailnet", "nocturne-tailnet")):
    main = f"{helpers}/{kind}/fake/{name}"
    try:
        digest = sha(main)
    except OSError:
        continue
    text += f'[[helpers]]\nkind = "{kind}"\nversion = "fake"\nlicence = "Apache-2.0"\npinned_at = "2000-01-01"\nbase_url = "https://example.invalid"\n'
    for platform in ("linux-x86_64", "linux-aarch64", "macos-aarch64", "windows-x86_64"):
        text += f'\n[helpers.assets.{platform}]\nfile = "{name if kind == "tailnet" else "fake"}"\nsha256 = "{digest}"\n'
    text += "\n"
open(out, "w").write(text)
PY
        export NOCTURNE_FAKE_HELPERS_LOCK=$scratch/fake.lock
    fi

    find_app() {
        for p in /proc/[0-9]*; do
            [[ "$(readlink "$p/exe" 2> /dev/null)" == "$procexe" ]] && { printf '%s' "${p#/proc/}"; return; }
        done
    }
    stop_app() {
        local pid
        pid=$(find_app)
        [[ -n "$pid" ]] && kill -TERM "$pid" 2> /dev/null
        for ((i = 0; i < 100; i++)); do
            [[ -z "$(find_app)" ]] && return
            sleep 0.1
        done
        pid=$(find_app)
        [[ -n "$pid" ]] && kill -KILL "$pid" 2> /dev/null
    }

    # Roots whose executables must be gone once the app has quit.
    roots=("$procexe" "$helpers_dir")
    if [[ -n "$bundle" ]]; then roots+=("$bundle"); else roots+=("$(dirname "$(dirname "$procexe")")"); fi
    # strace waits for every traced child, so a lingering engine would block it for good. Give it
    # 15 seconds, then stop what is left by pid and fail.
    reap() {
        local i p target r
        for ((i = 0; i < 150; i++)); do
            kill -0 "$first" 2> /dev/null || return 0
            sleep 0.1
        done
        for p in /proc/[0-9]*; do
            target=$(readlink "$p/exe" 2> /dev/null) || continue
            for r in "${roots[@]}"; do
                if [[ "$target" == "$r" || "$target" == "$r"/* ]]; then
                    bad "left running after quit, stopped by pid ${p#/proc/}: $target"
                    kill -KILL "${p#/proc/}" 2> /dev/null
                    break
                fi
            done
        done
        for ((i = 0; i < 50; i++)); do
            kill -0 "$first" 2> /dev/null || return 0
            sleep 0.1
        done
        bad "the trace did not end; stopped by pid $first"
        kill -KILL "$first" 2> /dev/null
    }

    logs=$scratch/data/logs
    if [[ "$mode" == online ]]; then
        marks=$scratch/marks.txt
        : > "$marks"
        export NET_CHECK_MARKS=$marks NET_SECONDS=$seconds NET_CHECK_PROVIDER_URL=https://$provider/v1
        timeout $((seconds + 1200)) strace -f -qq -ttt -e trace="$traced" -s 256 -o "$scratch/trace-1.log" \
            "$launch" --ignored --nocapture --test-threads=1 online_answers_in_the_network_check \
            > "$scratch/stderr/first.out" 2> "$scratch/stderr/first.err" &
    else
        trace "$scratch/trace-1.log" "$launch" > "$scratch/stderr/first.out" 2> "$scratch/stderr/first.err" &
    fi
    first=$!

    if [[ "$mode" == online ]]; then
        wait "$first"
        driver=$?
        reap
        if ((driver == 0)); then good "the driver finished"; else bad "the driver failed ($driver): $(tail -n 5 "$scratch/stderr/first.out" | tr '\n' ' ')"; fi
        for step in "engine ready" "ask 1:" "turned off" "ask after off 1:"; do
            if grep -q "$step" "$scratch/stderr/first.out"; then good "the driver reached '$step'"; else bad "the driver never reached '$step'"; fi
        done
        for kind in ask idle-start idle-end off; do
            if grep -q " $kind\$" "$marks"; then good "mark $kind written"; else bad "mark $kind missing"; fi
        done
    elif [[ "$mode" == attach ]]; then
        printf 'attached: use the app, then press Ctrl+C here to end and print the report\n' >&2
        trap 'stop_app' INT TERM
        wait "$first"
        trap - INT TERM
        stop_app
        reap
    else
        sleep "$seconds"
        if [[ -n "$(find_app)" ]]; then good "alive after ${seconds}s"; else bad "the app was not running after ${seconds}s"; fi
        if [[ "$mode" == remote && -n "${NET_CHECK_PHONE:-}" ]]; then
            if timeout 120 "$NET_CHECK_PHONE" "$scratch/data" > "$scratch/stderr/phone.out" 2>&1; then good "the scripted phone finished"; else bad "the scripted phone failed: $(tail -n 3 "$scratch/stderr/phone.out")"; fi
        fi
        if [[ "$mode" == prepared || "$mode" == remote ]]; then
            printf 'A note kept during the network check.\n' > "$scratch/note.txt"
            python3 -I - "$scratch/image.png" << 'PY'
import struct, sys, zlib
w = h = 64
raw = b"".join(b"\0" + b"".join(bytes([(x * 4) % 256, (y * 4) % 256, 128]) for x in range(w)) for y in range(h))
def chunk(kind, body):
    return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body) & 0xFFFFFFFF)
png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b"")
open(sys.argv[1], "wb").write(png)
PY
            timeout 60 "$(command -v strace)" -f -qq -e trace="$traced" -s 256 -o "$scratch/trace-2.log" "$launch" "$scratch/note.txt" "$scratch/image.png" > /dev/null 2>&1
        else
            printf 'A note kept during the network check.\n' > "$scratch/note.txt"
            timeout 60 "$(command -v strace)" -f -qq -e trace="$traced" -s 256 -o "$scratch/trace-2.log" "$launch" "$scratch/note.txt" > /dev/null 2>&1
        fi
        sleep 3
        stop_app
        reap
        wait "$first" 2> /dev/null
    fi

    if [[ "$mode" == online ]]; then
        :
    elif grep -qh 'window shown: interface ready' "$logs"/*.log 2> /dev/null; then
        good "the app worked: 'window shown: interface ready' logged"
    else
        bad "the app never showed its interface, so the trace proves nothing"
    fi
    engine=$(grep -Eho 'the private engine .* is ready on 127\.0\.0\.1:[0-9]+' "$logs"/*.log 2> /dev/null | head -1)
    if [[ -n "$engine" ]]; then
        port=${engine##*:}
        good "the app worked: $engine"
        if grep -Eqh "sin_port=htons\($port\), sin_addr=inet_addr\(\"127\.0\.0\.1\"\)" "$scratch"/trace-*.log 2> /dev/null; then
            good "the trace holds a connection to the engine on 127.0.0.1:$port"
        else
            bad "no traced connection to the engine's port $port, so the trace did not follow it"
        fi
    else
        bad "the private engine never started, so the trace proves nothing ($(grep -h 'private engine' "$logs"/*.log 2> /dev/null | tail -1))"
    fi
    if [[ "$mode" != attach ]]; then
        if grep -qh 'quit: everything stopped' "$logs"/*.log 2> /dev/null; then good "the app quit cleanly"; else bad "no 'quit: everything stopped' in the log"; fi
        if [[ "$mode" == online ]]; then
            :
        elif grep -qh 'second launch: kept' "$logs"/*.log 2> /dev/null; then
            good "the app worked: a second-launch file was kept"
        else
            bad "no 'second launch: kept' in the log, so the trace proves nothing"
        fi
    fi
    if grep -q 'panicked' "$logs"/*.log "$scratch"/stderr/* 2> /dev/null; then bad "a panic was logged"; fi

    if [[ "$mode" != remote && -d "$helpers_dir" ]] && [[ -n "$(find "$helpers_dir" -type f 2> /dev/null | head -1)" ]]; then
        bad "files under remote/helpers although nothing was consented: $(find "$helpers_dir" -type f | head -3 | tr '\n' ' ')"
    fi
    if [[ "$mode" == remote ]]; then
        for dir in $expected; do
            if [[ -s "$dir/fake-record.jsonl" ]]; then good "the stand-in ran: $(basename "$(dirname "$dir")")"; else bad "the stand-in never ran in $dir"; fi
        done
    fi
    judge=(python3 -I "$repo/scripts/netlog.py" --scratch "$scratch" --mode "$on_or_off"
        --helpers-dir "$helpers_dir/" --app-exe "$procexe")
    [[ -n "$bundle" ]] && judge+=(--lemond-dir "$bundle/")
    [[ "$mode" == remote ]] && judge+=(--require-helper-traffic)
    [[ "$mode" == online ]] && judge+=(--provider "$provider" --marks "$marks" --require-provider-traffic)
    {
        printf -- '--- outbound report (%s, %s)\n' "$label" "$mode"
        "${judge[@]}" "$scratch"/trace-*.log
    } >> "$result"
    "${judge[@]}" "$scratch"/trace-*.log > /dev/null || fail=1
    exit "$fail"
fi

# The scripted driver for the online mode: a test build of the app, never packaged.
online_driver() {
    cargo test -p nocturne-app --test netcheck_script --no-run --message-format=json 2> /dev/null |
        python3 -I -c '
import json, sys
for line in sys.stdin:
    try:
        m = json.loads(line)
    except ValueError:
        continue
    if m.get("reason") == "compiler-artifact" and m.get("executable") and m["target"]["name"] == "netcheck_script":
        print(m["executable"])
'
}

mode=${1:-launch}
variant=${2:-binary}
[[ "$mode" == online ]] && variant=binary
case "$mode" in launch | prepared | remote | online | attach) ;; *) printf 'unknown mode: %s\n' "$mode" >&2; exit 2 ;; esac
for tool in unshare strace ip xvfb-run dbus-run-session; do
    command -v "$tool" > /dev/null || { printf 'missing tool: %s\n' "$tool" >&2; exit 2; }
done
unshare -rn true 2> /dev/null || { printf 'a user and network namespace is not available here (unshare -rn)\n' >&2; exit 2; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
scratch=$work/$variant
mkdir -p "$scratch"
bundle=""
case "$variant" in
    binary)
        if [[ "$mode" == online ]]; then
            launch=$(online_driver)
            [[ -n "$launch" ]] || { printf 'could not build the scripted driver\n' >&2; exit 2; }
        else
            launch=${NET_CHECK_BINARY:-$repo/target/release/nocturne}
        fi
        procexe=$(realpath -- "$launch")
        bundle=$pkg/lemonade/linux-x86_64
        [[ -d "$bundle" ]] || bundle=$repo/vendor/lemonade/linux-x86_64
        ;;
    appimage)
        f=$(ls "$pkg"/*.AppImage 2> /dev/null | head -1)
        [[ -n "$f" ]] || { printf 'no AppImage in %s\n' "$pkg" >&2; exit 2; }
        cp -- "$f" "$scratch/app.AppImage" && chmod +x "$scratch/app.AppImage"
        (cd "$scratch" && APPIMAGE_EXTRACT_AND_RUN=1 ./app.AppImage --appimage-extract > /dev/null)
        launch=$scratch/squashfs-root/AppRun
        procexe=$scratch/squashfs-root/usr/bin/nocturne
        ;;
    deb | rpm)
        f=$(ls "$pkg"/*."$variant" 2> /dev/null | head -1)
        [[ -n "$f" ]] || { printf 'no %s in %s\n' "$variant" "$pkg" >&2; exit 2; }
        mkdir -p "$scratch/root"
        if [[ "$variant" == deb ]]; then
            (cd "$scratch" && ar x "$f" && tar -xf data.tar.* -C root)
        else
            rpm2archive -n - < "$f" | tar -xf - -C "$scratch/root"
        fi
        launch=$scratch/root/usr/bin/nocturne
        procexe=$launch
        ;;
    *) printf 'unknown variant: %s\n' "$variant" >&2; exit 2 ;;
esac
[[ -e "$launch" ]] || { printf 'nothing to run at %s\n' "$launch" >&2; exit 2; }
procexe=$(realpath -- "$procexe")

printf '== net-check %s on %s\n' "$mode" "$variant"
# The scratch environment is set before the namespace, the display and the session bus start,
# so every service they wake uses it and none sees the person's real home or runtime folder.
home=$scratch/home
mkdir -p "$home" "$scratch/runtime" "$scratch/data"
chmod 700 "$scratch/runtime"
export HOME=$home XDG_CONFIG_HOME=$home/.config XDG_DATA_HOME=$home/.local/share \
    XDG_CACHE_HOME=$home/.cache XDG_RUNTIME_DIR=$scratch/runtime NOCTURNE_DATA_DIR=$scratch/data
unset NOCTURNE_LEMOND
[[ -n "$bundle" ]] && export NOCTURNE_LEMOND=$bundle
if [[ "$mode" == attach ]]; then
    # Real display and sound, by socket path (see the usage note).
    unset WAYLAND_DISPLAY PULSE_SERVER DISPLAY
    [[ -n "${NET_CHECK_WAYLAND:-}" ]] && export WAYLAND_DISPLAY=$NET_CHECK_WAYLAND
    [[ -n "${NET_CHECK_PULSE:-}" ]] && export PULSE_SERVER=$NET_CHECK_PULSE
    [[ -n "${NET_CHECK_DISPLAY:-}" ]] && export DISPLAY=$NET_CHECK_DISPLAY
else
    export LIBGL_ALWAYS_SOFTWARE=1 WEBKIT_DISABLE_DMABUF_RENDERER=1 WEBKIT_DISABLE_COMPOSITING_MODE=1
fi
set +e
unshare -rn "$repo/scripts/net-check.sh" --in-ns "$mode" "$launch" "$procexe" "$bundle" "$scratch" "$variant" > "$scratch/session.log" 2>&1
code=$?
set -e
cat "$scratch/result.txt" 2> /dev/null
if ((code != 0)); then
    tail -n 20 "$scratch/session.log"
    printf 'net-check: failed\n'
else
    printf 'net-check: passed\n'
fi
exit "$code"
