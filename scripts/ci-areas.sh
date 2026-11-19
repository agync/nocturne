#!/usr/bin/env bash
# Which jobs of .github/workflows/ci.yml a change needs.
#
# Usage: scripts/ci-areas.sh < changed-paths   (any other argument is refused)
#        scripts/ci-areas.sh --all
#
# Reads the changed paths, one per line, and prints one `area=true|false` line per area, in a
# fixed order. A change that matches `everything`, and --all, print every area true. Each area
# lists the paths its jobs read, including the files one side's tests read from the other.
set -euo pipefail
export LC_ALL=C

# A change here runs every job.
everything='^(\.github/(workflows|actions)/|Makefile$|rust-toolchain\.toml$|scripts/ci-areas\.sh$)'
lockfiles='Cargo\.(toml|lock)$|(helpers|lemonade)\.lock$'
rust_side="crates/|src-tauri/|docs/guide/|\.cargo/|clippy\.toml$|$lockfiles"
# What the packages are built from. The Rust sources are not here: the rust and rust-platforms
# jobs check their release build.
package="scripts/|src-tauri/(bundle/|macos/|icons/|tauri[^/]*\.json$|Info\.plist$|Cargo\.toml$|build\.rs$)|Cargo\.(toml|lock)$|LICENSE$|ui/package[^/]*\.json$|(helpers|lemonade)\.lock$|\.github/workflows/release\.yml$"

names=(rust rust-platforms ui ui-platforms scripts tailnet site package package-linux)
declare -A areas=(
    # The Rust tests read ui/ and write the bindings there, and the interop tests run the phone
    # client from ui/ against the Rust side.
    [rust]="^($rust_side|ui/|scripts/(bindings|away-integration|away-test-programs)\.sh$)"
    # The Windows job starts the real engine that scripts/fetch-lemonade.sh fetches.
    [rust-platforms]="^($rust_side|scripts/fetch-lemonade\.sh$)"
    # The interface's tests import scripts/build-tokens.mjs and read src-tauri/.
    [ui]='^(ui/|src-tauri/|scripts/(build-tokens\.mjs|check_ui_dist\.py|check_tree\.py)$)'
    [ui-platforms]='^(ui/|scripts/build-tokens\.mjs$)'
    # The scripts' tests read the bundle configuration, the Flatpak files and their doc, and list
    # the licences of every crate and npm package.
    [scripts]="^(scripts/|$rust_side|flatpak/|docs/contribution/flatpak\.md$|LICENSE$|ui/package(-lock)?\.json$)"
    [tailnet]='^(helpers/|scripts/(test-tailnet|tailnet-tags|build-tailnet)\.sh$)'
    [site]='^(site/|docs/|lemonade\.lock$|ui/package(-lock)?\.json$)'
    [package]="^($package)"
    [package-linux]="^($package|flatpak/)"
)

# grep exits 1 when nothing matches and 2 on a bad pattern, which must fail rather than skip.
matches() {
    local status=0
    grep -Eq "$1" "$2" || status=$?
    [ "$status" -le 1 ] || exit "$status"
    return "$status"
}

changed=$(mktemp)
trap 'rm -f "$changed"' EXIT
all=false
case "$#:${1:-}" in
    1:--all) all=true ;;
    0:)
        cat > "$changed"
        if matches "$everything" "$changed"; then all=true; fi
        ;;
    *)
        printf 'usage: %s [--all] < changed-paths\n' "$0" >&2
        exit 2
        ;;
esac

for area in "${names[@]}"; do
    run=$all
    if [ "$run" = false ] && matches "${areas[$area]}" "$changed"; then run=true; fi
    echo "$area=$run"
done
