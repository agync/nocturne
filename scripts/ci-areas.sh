#!/usr/bin/env bash
# Which jobs of .github/workflows/ci.yml a change needs.
#
# Usage: scripts/ci-areas.sh < changed-paths   (any other argument is refused)
#        scripts/ci-areas.sh --all
#        scripts/ci-areas.sh --local < changed-paths
#
# Reads the changed paths, one per line, and prints one `area=true|false` line per area, in a
# fixed order. A change that matches `everything`, and --all, print every area true. Each area
# lists the paths its jobs read, including the files one side's tests read from the other.
# --local reads the same table but ignores `everything`: scripts/check-changed.sh uses it to pick
# the quick checks for the paths that changed, and leaves the run-everything rule to CI.
set -euo pipefail
export LC_ALL=C

# A change here runs every job.
everything='^(\.github/(workflows|actions)/|Makefile$|rust-toolchain\.toml$|scripts/ci-areas\.sh$)'
lockfiles='Cargo\.(toml|lock)$|(helpers|lemonade)\.lock$'
rust_side="crates/|src-tauri/|phone/|docs/guide/|\.cargo/|clippy\.toml$|$lockfiles"
# What the packages are built from. The Rust sources are not here: the rust and rust-platforms
# jobs check their release build.
package="scripts/|src-tauri/(bundle/|macos/|icons/|tauri[^/]*\.json$|Info\.plist$|Cargo\.toml$|build\.rs$)|Cargo\.(toml|lock)$|LICENSE$|ui/package[^/]*\.json$|(helpers|lemonade)\.lock$|\.github/workflows/release\.yml$"

names=(rust rust-platforms ui ui-platforms scripts tailnet site package package-linux android phone-engine)
declare -A areas=(
    # The Rust tests read ui/ and write the bindings there, and the interop tests run the phone
    # client from ui/ against the Rust side.
    [rust]="^($rust_side|ui/|scripts/(bindings|away-integration|away-test-programs)\.sh$)"
    # The Windows job starts the real engine that scripts/fetch-lemonade.sh fetches.
    [rust-platforms]="^($rust_side|scripts/fetch-lemonade\.sh$)"
    # The interface's tests import scripts/build-tokens.mjs and read src-tauri/ and the channel's
    # word list.
    [ui]='^(ui/|src-tauri/|crates/channel/|scripts/(build-tokens\.mjs|check_ui_dist\.py|check_tree\.py)$)'
    [ui-platforms]='^(ui/|scripts/build-tokens\.mjs$)'
    # The scripts' tests read the bundle configuration, the Flatpak files and their doc, and list
    # the licences of every crate and npm package.
    [scripts]="^(scripts/|$rust_side|flatpak/|docs/contribution/flatpak\.md$|LICENSE$|ui/package(-lock)?\.json$)"
    [tailnet]='^(helpers/|scripts/(test-tailnet|tailnet-tags|build-tailnet)\.sh$)'
    [site]='^(site/|docs/|lemonade\.lock$|ui/package(-lock)?\.json$)'
    [package]="^($package)"
    [package-linux]="^($package|flatpak/)"
    # The phone app: its shell, the channel it links, and the phone screens it carries.
    [android]='^(phone/|crates/channel/|ui/src/phone/|ui/phone/|ui/vite\.phone\.config\.ts$|ui/package(-lock)?\.json$|Cargo\.(toml|lock)$)'
    # The engine builds for the phone: a check of the engine alone, with no APK.
    [phone-engine]='^(crates/(contract|engine)/|Cargo\.(toml|lock)$|\.cargo/)'
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
    1:--local) cat > "$changed" ;;
    0:)
        cat > "$changed"
        if matches "$everything" "$changed"; then all=true; fi
        ;;
    *)
        printf 'usage: %s [--all|--local] < changed-paths\n' "$0" >&2
        exit 2
        ;;
esac

for area in "${names[@]}"; do
    run=$all
    if [ "$run" = false ] && matches "${areas[$area]}" "$changed"; then run=true; fi
    echo "$area=$run"
done
