#!/usr/bin/env bash
# Write the offline sources the Flatpak build needs into flatpak/generated/.
#
# Usage: scripts/flatpak-sources.sh
#
# cargo-sources.json comes from Cargo.lock and node-sources.json from ui/package-lock.json, with
# the generators of flatpak-builder-tools at a pinned commit. Both files are generated, never
# committed, so they cannot drift from the lockfiles. Needs uv and network access to fetch the
# generators and to read the package registries; nothing is installed outside uv's own cache.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

tools_commit=9d86d336a39b9af38b86f114b6ce5820fe260f91
tools_repo=https://github.com/flatpak/flatpak-builder-tools
cargo_generator_sha256=0a2db6be87d75910facef28ab46d4d6460802e8419ab850d0caa6a364d26b380
out=flatpak/generated

command -v uv > /dev/null || { printf 'uv is needed: https://docs.astral.sh/uv/\n' >&2; exit 2; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$out" "$work/tools" "$work/node"

curl --fail --location --silent --show-error \
    -o "$work/tools/flatpak-cargo-generator.py" \
    "https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/$tools_commit/cargo/flatpak-cargo-generator.py"
printf '%s  %s\n' "$cargo_generator_sha256" "$work/tools/flatpak-cargo-generator.py" | sha256sum --check --quiet

printf '==> cargo-sources.json\n'
uv run --quiet --no-project --script "$work/tools/flatpak-cargo-generator.py" Cargo.lock -o "$out/cargo-sources.json"

# The node generator must not see a node_modules folder, so it reads a copy of the lockfile.
printf '==> node-sources.json\n'
cp ui/package.json ui/package-lock.json "$work/node/"
uv tool run --quiet --from "git+$tools_repo@$tools_commit#subdirectory=node" \
    flatpak-node-generator npm "$work/node/package-lock.json" -o "$out/node-sources.json"

ls -l "$out"
