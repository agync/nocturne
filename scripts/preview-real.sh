#!/usr/bin/env bash
# Run the development-only real preview: the real app, engine and models behind HTTP on loopback,
# for use from a browser. Never shipped; `make preview-real` builds first and calls this.
#
#   PREVIEW_DATA_DIR      its own data folder (default target/preview-real), never the app's
#   PREVIEW_PORT          the loopback port (default 55066)
#   PREVIEW_PUBLIC_ORIGIN where other machines reach it, such as https://host.example:8443
#   PREVIEW_BIN, PREVIEW_UI  a copy of the server and of the built interface to run instead of the
#                         ones in target/ and ui/dist-preview (so a rebuild never touches a live one)
#   PREVIEW_MODELS_FROM   the engine folder of a data folder that already finished setup: its
#                         models and engine files are hard-linked in, so nothing downloads
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

data=${PREVIEW_DATA_DIR:-$PWD/target/preview-real}
port=${PREVIEW_PORT:-55066}
mkdir -p "$data"
# Only an empty folder is claimed here; the server refuses any other that it did not mark.
[[ -e "$data/.nocturne-preview" || -n "$(ls -A "$data")" ]] || : > "$data/.nocturne-preview"

from=${PREVIEW_MODELS_FROM:-}
if [[ -n "$from" && ! -e "$data/engine/models" ]]; then
    [[ -d "$from/models" ]] || { printf 'no models in %s\n' "$from" >&2; exit 2; }
    mkdir -p "$data/engine/config"
    cp -al -- "$from/models" "$data/engine/models"
    [[ -d "$from/cache" ]] && cp -al -- "$from/cache" "$data/engine/cache"
    # Which mind is set up. The engine's own settings are written afresh for this folder.
    [[ -f "$from/config/nocturne-setup.json" ]] \
        && cp -- "$from/config/nocturne-setup.json" "$data/engine/config/nocturne-setup.json"
fi

args=(--data-dir "$data" --port "$port" --ui "${PREVIEW_UI:-$PWD/ui/dist-preview}")
[[ -n "${PREVIEW_PUBLIC_ORIGIN:-}" ]] && args+=(--public-origin "$PREVIEW_PUBLIC_ORIGIN")
exec "${PREVIEW_BIN:-target/debug/nocturne-preview}" "${args[@]}"
