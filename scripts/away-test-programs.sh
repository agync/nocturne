# Sourced by scripts/away-integration.sh and scripts/away-trials.sh.

# Build the named test programs of the app with the stand-in helpers and export each one's path
# as AWAY_<NAME> for ui/interop/driver.ts. Cargo reports the paths, since where it keeps test
# programs depends on its version.
build_tests() {
    local args=() name programs program
    for name in "$@"; do args+=(--test "$name"); done
    programs=$(cargo test -p nocturne-app --features fake-helpers "${args[@]}" --no-run \
        --message-format=json-render-diagnostics |
        python3 -I -c '
import json, sys
wanted = set(sys.argv[1:])
for line in sys.stdin:
    m = json.loads(line)
    if m.get("reason") == "compiler-artifact" and m.get("executable") and m["profile"]["test"]:
        if m["target"]["name"] in wanted:
            print(m["target"]["name"] + "=" + m["executable"])
' "$@")
    while IFS='=' read -r name program; do
        [[ -n "$name" ]] && export "AWAY_$(printf %s "$name" | tr '[:lower:]' '[:upper:]')=$program"
    done <<< "$programs"
    for name in "$@"; do
        [[ "$programs" == *"$name="* ]] || { printf 'cargo reported no test program %s\n' "$name" >&2; exit 1; }
    done
}
