#!/usr/bin/env bash
# Launch each built Nocturne package under a virtual display and prove it starts cleanly.
#
# Usage: scripts/smoke-launch.sh [binary|appimage|deb|rpm|all]      (default: all)
#   SMOKE_SECONDS   how long the app runs before it is judged (default 30)
#   SMOKE_BINARY    the release binary to use for "binary" (default target/release/nocturne)
#
# Each run gets a scratch home, XDG folders, runtime folder and data folder, so nothing touches
# the person's own files. Pass means: the app is alive at the end; the log has "window shown:
# interface ready" and no panic; a second launch with a text file exits 0 and the first
# instance keeps it; SIGTERM ends the app within 10 seconds; and no process whose executable
# lives in the package's files is left behind. Processes are stopped by the pid recorded here.
set -uo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
repo=$PWD
pkg=$repo/target/package
seconds=${SMOKE_SECONDS:-30}

# Inner mode: runs inside the virtual display and the private session bus.
if [[ "${1:-}" == --inner ]]; then
    shift
    exe=$1 procexe=$2 bundle=$3 scratch=$4 label=$5
    shift 5
    # The scratch environment was exported before the session bus started (see below), so the
    # services it wakes use it too.
    mkdir -p "$scratch/stderr"
    fail=0
    result=$scratch/result.txt
    bad() { printf 'FAIL  %s: %s\n' "$label" "$*" >> "$result"; fail=1; }
    good() { printf 'ok    %s: %s\n' "$label" "$*" >> "$result"; }
    info() { printf 'info  %s: %s\n' "$label" "$*" >> "$result"; }

    "$exe" > "$scratch/stderr/first.out" 2> "$scratch/stderr/first.err" &
    first=$!
    sleep "$seconds"

    if kill -0 "$first" 2> /dev/null; then good "alive after ${seconds}s"; else bad "exited before ${seconds}s"; fi

    # The app's own pid: the process whose executable is the binary (an AppImage starts it
    # through AppRun, which has its own pid until it hands over).
    app=""
    for p in /proc/[0-9]*; do
        [[ "$(readlink "$p/exe" 2> /dev/null)" == "$procexe" ]] && app=${p#/proc/} && break
    done
    [[ -n "$app" ]] || app=$first
    info "app pid $app"

    logs=$scratch/data/logs
    if grep -qh 'window shown: interface ready' "$logs"/*.log 2> /dev/null; then
        good "log has 'window shown: interface ready'"
    else
        bad "no 'window shown: interface ready' in the log ($(grep -h 'window shown' "$logs"/*.log 2> /dev/null | tail -1))"
    fi
    if grep -Eqh 'the private engine .* is ready on 127\.0\.0\.1:[0-9]+' "$logs"/*.log 2> /dev/null; then
        good "the private engine started ($(grep -Eho 'the private engine .* is ready on 127\.0\.0\.1:[0-9]+' "$logs"/*.log | head -1 | sed 's/:[0-9]*$//'))"
    else
        bad "the private engine never started ($(grep -h 'private engine' "$logs"/*.log 2> /dev/null | tail -1))"
    fi
    if grep -q 'panicked' "$logs"/*.log "$scratch"/stderr/* 2> /dev/null; then bad "a panic was logged"; else good "no panic in the log or stderr"; fi

    printf 'A note kept by the second launch.\n' > "$scratch/second-launch-note.txt"
    timeout 30 "$exe" "$scratch/second-launch-note.txt" > "$scratch/stderr/second.out" 2> "$scratch/stderr/second.err"
    second=$?
    if ((second == 0)); then good "second launch exited 0"; elif ((second == 124)); then bad "second launch did not return within 30s"; else bad "second launch exited $second"; fi
    sleep 3
    if grep -qh 'second launch: kept 1 files' "$logs"/*.log 2> /dev/null; then
        good "the first instance logged the kept file"
    else
        bad "no 'second launch: kept 1 files' in the log"
    fi

    kill -TERM "$app" 2> /dev/null
    stopped=""
    for ((i = 0; i < 100; i++)); do
        kill -0 "$app" 2> /dev/null || { stopped=$i; break; }
        sleep 0.1
    done
    if [[ -n "$stopped" ]]; then good "SIGTERM ended the app in $((stopped / 10)).$((stopped % 10))s"; else bad "still running 10s after SIGTERM"; kill -KILL "$app" 2> /dev/null; fi
    wait "$first" 2> /dev/null
    sleep 1
    if grep -qh 'quit: everything stopped' "$logs"/*.log 2> /dev/null; then good "the app logged 'quit: everything stopped'"; else bad "no 'quit: everything stopped' in the log"; fi

    # Roots whose executables must be gone: the engine's folder and the package's files.
    roots=("$procexe")
    if [[ -n "$bundle" ]]; then roots+=("$bundle"); else roots+=("$(dirname "$(dirname "$procexe")")"); fi
    left=()
    for p in /proc/[0-9]*; do
        target=$(readlink "$p/exe" 2> /dev/null) || continue
        for r in "${roots[@]}"; do
            if [[ "$target" == "$r" || "$target" == "$r"/* ]]; then left+=("${p#/proc/}:$target"); break; fi
        done
    done
    if ((${#left[@]} == 0)); then
        good "no process from the package is left"
    else
        bad "left behind (stopped by pid): ${left[*]}"
        for entry in "${left[@]}"; do kill -KILL "${entry%%:*}" 2> /dev/null; done
    fi
    exit "$fail"
fi

variant=${1:-all}
variants=("$variant")
[[ "$variant" == all ]] && variants=(binary appimage deb rpm)
for tool in xvfb-run dbus-run-session; do
    command -v "$tool" > /dev/null || { printf 'missing tool: %s\n' "$tool" >&2; exit 2; }
done

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
verdict=0
for v in "${variants[@]}"; do
    scratch=$work/$v
    mkdir -p "$scratch"
    bundle=""
    case "$v" in
        binary)
            exe=${SMOKE_BINARY:-$repo/target/release/nocturne}
            procexe=$(realpath -- "$exe")
            bundle=$pkg/lemonade/linux-x86_64
            [[ -d "$bundle" ]] || bundle=$repo/vendor/lemonade/linux-x86_64
            ;;
        appimage)
            f=$(ls "$pkg"/*.AppImage 2> /dev/null | head -1)
            [[ -n "$f" ]] || { printf 'SKIP  appimage: none in %s\n' "$pkg"; verdict=1; continue; }
            cp -- "$f" "$scratch/app.AppImage" && chmod +x "$scratch/app.AppImage"
            (cd "$scratch" && APPIMAGE_EXTRACT_AND_RUN=1 ./app.AppImage --appimage-extract > /dev/null)
            # Started through AppRun, as the AppImage starts itself, so the bundled libraries
            # and their environment are the ones under test.
            exe=$scratch/squashfs-root/AppRun
            procexe=$scratch/squashfs-root/usr/bin/nocturne
            ;;
        deb)
            f=$(ls "$pkg"/*.deb 2> /dev/null | head -1)
            [[ -n "$f" ]] || { printf 'SKIP  deb: none in %s\n' "$pkg"; verdict=1; continue; }
            mkdir -p "$scratch/root"
            (cd "$scratch" && ar x "$f" && tar -xf data.tar.* -C root)
            exe=$scratch/root/usr/bin/nocturne
            procexe=$exe
            ;;
        rpm)
            f=$(ls "$pkg"/*.rpm 2> /dev/null | head -1)
            [[ -n "$f" ]] || { printf 'SKIP  rpm: none in %s\n' "$pkg"; verdict=1; continue; }
            mkdir -p "$scratch/root"
            (cd "$scratch/root" && rpm2cpio "$f" | cpio -idm --quiet)
            exe=$scratch/root/usr/bin/nocturne
            procexe=$exe
            ;;
        *) printf 'unknown variant: %s\n' "$v" >&2; exit 2 ;;
    esac
    [[ -x "$exe" ]] || { printf 'FAIL  %s: no executable at %s\n' "$v" "$exe"; verdict=1; continue; }
    # Installed packages find the engine among their own files; only the bare binary needs help.
    printf '== %s (%s)\n' "$v" "$(basename "$exe")"
    # The scratch environment is set here, before the session bus starts, so the helper
    # services the bus wakes use it and never see the person's real home or runtime folder.
    home=$scratch/home
    mkdir -p "$home" "$scratch/runtime" "$scratch/data"
    chmod 700 "$scratch/runtime"
    export HOME=$home XDG_CONFIG_HOME=$home/.config XDG_DATA_HOME=$home/.local/share \
        XDG_CACHE_HOME=$home/.cache XDG_RUNTIME_DIR=$scratch/runtime NOCTURNE_DATA_DIR=$scratch/data
    export LIBGL_ALWAYS_SOFTWARE=1 WEBKIT_DISABLE_DMABUF_RENDERER=1 WEBKIT_DISABLE_COMPOSITING_MODE=1
    unset NOCTURNE_LEMOND
    [[ -n "$bundle" ]] && export NOCTURNE_LEMOND=$bundle
    # The private session bus wakes the desktop's helper services and they are noisy; their
    # output is kept in session.log and shown only when the run fails.
    if ! xvfb-run -a -s "-screen 0 1280x800x24" dbus-run-session -- "$repo/scripts/smoke-launch.sh" \
        --inner "$exe" "$(realpath -- "$procexe")" "$bundle" "$scratch" "$v" > "$scratch/session.log" 2>&1; then
        verdict=1
        tail -n 20 "$scratch/session.log"
    fi
    cat "$scratch/result.txt" 2> /dev/null
done
if ((verdict == 0)); then printf 'smoke: all passed\n'; else printf 'smoke: failed\n'; fi
exit "$verdict"
