#!/usr/bin/env bash
# Launch the built Nocturne.app on a Mac and prove it starts and stops cleanly.
#
# Usage: scripts/smoke-launch-macos.sh [PATH/TO/Nocturne.app]   (default target/package/macos/Nocturne.app)
#   SMOKE_SECONDS   how long the app runs before it is judged (default 20)
#
# The app is started with `open`, in the logged-in user's graphical session, so it opens a real
# window on that screen (from ssh too, while that user is logged in at the Mac). It gets a scratch data folder and home; the person's own
# Nocturne data is never touched. Pass means: alive at the end; the log has the engine ready on
# 127.0.0.1, "setup is Choosing", "window shown: interface ready" and the interface's one line
# on whether its web view can record, and no panic; a second
# launch with a text file exits 0 and the first instance keeps it; SIGTERM ends the app within
# 10 seconds with "quit: everything stopped"; no process from the app's files is left; and
# ~/Library/LaunchAgents and the web view's folders under ~/Library are exactly as they were. Processes are stopped by the pid found here.
set -uo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
[[ "$(uname -s)" == Darwin ]] || { printf 'this runs on a Mac\n' >&2; exit 2; }

app=$(cd "$(dirname "${1:-target/package/macos/Nocturne.app}")" && pwd -P)/$(basename "${1:-target/package/macos/Nocturne.app}")
exe=$app/Contents/MacOS/Nocturne
[[ -x "$exe" ]] || { printf 'no executable at %s\n' "$exe" >&2; exit 2; }
seconds=${SMOKE_SECONDS:-20}

scratch=$(mktemp -d "${TMPDIR:-/tmp}/nocturne-smoke.XXXXXX")
mkdir -p "$scratch/home" "$scratch/data" "$scratch/out"
logs=$scratch/data/logs
agents=$HOME/Library/LaunchAgents
agents_before=$(ls -la "$agents" 2> /dev/null | sed 1d)
# Foundation finds the home folder by CFFIXED_USER_HOME, then the account, not by HOME: with it
# set, the web view keeps its data in the scratch home and the real ~/Library is left alone.
real_webkit=("$HOME/Library/WebKit/app.nocturne.desktop" "$HOME/Library/Caches/app.nocturne.desktop")
webkit_before=$(ls -d "${real_webkit[@]}" 2> /dev/null)
fail=0
bad() { printf 'FAIL  %s\n' "$*"; fail=1; }
good() { printf 'ok    %s\n' "$*"; }
start() {
    open -n -a "$app" --env HOME="$scratch/home" --env CFFIXED_USER_HOME="$scratch/home" \
        --env NOCTURNE_DATA_DIR="$scratch/data" \
        --stdout "$scratch/out/first.out" --stderr "$scratch/out/first.err"
}
app_pid() { pgrep -f "^$exe\$" | head -1; }

start
sleep "$seconds"
pid=$(app_pid)
if [[ -n "$pid" ]]; then good "alive after ${seconds}s (pid $pid)"; else bad "exited before ${seconds}s"; fi

for pattern in "the private engine .* is ready on 127\.0\.0\.1:[0-9]+" "setup is Choosing" "window shown: interface ready" "recording: (available|unavailable)"; do
    line=$(grep -Ehm1 "$pattern" "$logs"/*.log 2> /dev/null | sed -E 's/^[^ ]+ +//')
    if [[ -n "$line" ]]; then good "log: $line"; else bad "no '$pattern' in the log"; fi
done
if grep -q 'panicked' "$logs"/*.log "$scratch"/out/* 2> /dev/null; then bad "a panic was logged"; else good "no panic in the log or stderr"; fi

printf 'A note kept by the second launch.\n' > "$scratch/second-launch-note.txt"
# The second launch is the binary itself: it hands the file to the running app and exits.
HOME="$scratch/home" CFFIXED_USER_HOME="$scratch/home" NOCTURNE_DATA_DIR="$scratch/data" "$exe" "$scratch/second-launch-note.txt" \
    > "$scratch/out/second.out" 2> "$scratch/out/second.err" &
second_pid=$!
for ((i = 0; i < 300; i++)); do kill -0 "$second_pid" 2> /dev/null || break; sleep 0.1; done
if kill -0 "$second_pid" 2> /dev/null; then
    bad "second launch did not return within 30s"
    kill -KILL "$second_pid" 2> /dev/null
elif wait "$second_pid"; then
    good "second launch exited 0"
else
    bad "second launch exited non-zero"
fi
sleep 3
if grep -qh 'second launch: kept 1 files' "$logs"/*.log 2> /dev/null; then good "the first instance logged the kept file"; else bad "no 'second launch: kept 1 files' in the log"; fi

if [[ -n "$pid" ]]; then
    kill -TERM "$pid" 2> /dev/null
    stopped=""
    for ((i = 0; i < 100; i++)); do
        kill -0 "$pid" 2> /dev/null || { stopped=$i; break; }
        sleep 0.1
    done
    if [[ -n "$stopped" ]]; then good "SIGTERM ended the app in $((stopped / 10)).$((stopped % 10))s"; else bad "still running 10s after SIGTERM"; kill -KILL "$pid" 2> /dev/null; fi
fi
wait 2> /dev/null
sleep 1
if grep -qh 'quit: everything stopped' "$logs"/*.log 2> /dev/null; then good "the app logged 'quit: everything stopped'"; else bad "no 'quit: everything stopped' in the log"; fi

left=$(ps -axo pid=,command= | grep -F "$app/" | grep -v grep | awk '{print $1}')
if [[ -z "$left" ]]; then
    good "no process from the app is left"
else
    bad "left behind (stopped by pid): $left"
    for p in $left; do kill -KILL "$p" 2> /dev/null; done
fi
if [[ "$(ls -d "${real_webkit[@]}" 2> /dev/null)" == "$webkit_before" ]]; then good "the real ~/Library/WebKit and Caches are untouched"; else bad "the app wrote its web view data under the real ~/Library"; fi
if [[ "$(ls -la "$agents" 2> /dev/null | sed 1d)" == "$agents_before" ]]; then good "~/Library/LaunchAgents is unchanged"; else bad "~/Library/LaunchAgents changed"; fi

if ((fail)); then printf 'smoke: failed (scratch kept: %s)\n' "$scratch"; else rm -rf "$scratch"; printf 'smoke: all passed\n'; fi
exit "$fail"
