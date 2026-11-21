#!/usr/bin/env bash
# The basics for what changed: the lint and drift checks of the areas the change touches, and the
# cheap hooks on the changed files. CI runs the full suite (docs/contribution/guide.md).
#
# Usage: scripts/check-changed.sh [--dry-run] [--paths FILE]
#
# BASE is the commit to compare with (default: the merge base of HEAD and main). The paths are
# the files that differ from BASE in the working tree, plus untracked ones, or the lines of FILE.
# TESTS is a command to run last, for the tests of the change: TESTS="cargo test -p x journal::".
# --dry-run prints what would run and runs nothing.
set -euo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."

dry=false
paths_file=""
while [ "$#" -gt 0 ]; do
    case "$1" in
        --dry-run) dry=true ;;
        --paths)
            [ "$#" -ge 2 ] || { echo "--paths needs a file" >&2; exit 2; }
            paths_file=$2
            shift
            ;;
        *)
            printf 'usage: %s [--dry-run] [--paths FILE]\n' "$0" >&2
            exit 2
            ;;
    esac
    shift
done

changed=$(mktemp)
trap 'rm -f "$changed"' EXIT
if [ -n "$paths_file" ]; then
    cp "$paths_file" "$changed"
else
    base=${BASE:-$(git merge-base HEAD main)}
    {
        git diff --name-only --diff-filter=d "$base"
        git ls-files --others --exclude-standard
    } | sort -u > "$changed"
fi

declare -A on
while IFS='=' read -r area flag; do
    on[$area]=$flag
done < <(scripts/ci-areas.sh --local < "$changed")

targets=()
add() { targets+=("$1"); }
[ "${on[rust]}" = true ] && { add rust/lint; add contract/check; }
[ "${on[ui]}" = true ] && add ui/lint
[ "${on[scripts]}" = true ] && add scripts/test
[ "${on[tailnet]}" = true ] && add tailnet/test
[ "${on[site]}" = true ] && { add docs/check; add site/strings; }
# The phone app's host clippy only: Gradle and the APK are CI's.
[ "${on[android]}" = true ] && add phone/lint

echo "changed: $(wc -l < "$changed") path(s)"
echo "targets: ${targets[*]:-none}"

run() {
    echo "+ $*"
    if [ "$dry" = false ]; then "$@"; fi
}

for t in "${targets[@]}"; do run make "$t"; done
if [ -s "$changed" ]; then
    if [ "$dry" = true ]; then
        echo "+ pre-commit run --files <changed files>"
    else
        existing=()
        while IFS= read -r f; do [ -e "$f" ] && existing+=("$f"); done < "$changed"
        if [ "${#existing[@]}" -gt 0 ]; then run pre-commit run --files "${existing[@]}"; fi
    fi
fi
if [ -n "${TESTS:-}" ]; then run bash -c "$TESTS"; fi
