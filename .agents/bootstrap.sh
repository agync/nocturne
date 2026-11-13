#!/usr/bin/env bash
# Idempotent local setup: git hooks, the scratch area, and git-ignored tool shims.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

if command -v pre-commit >/dev/null 2>&1; then
    pre-commit install --install-hooks --hook-type pre-commit --hook-type commit-msg
else
    printf '  pre-commit not found; hooks not installed\n'
fi

mkdir -p .agents/brain/inbox .agents/brain/outbox .agents/brain/tasks .agents/brain/assets

# Tool shims point every assistant at the committed contract. They are git-ignored.
for shim in CLAUDE.md GEMINI.md; do
    printf '@AGENTS.md\n' > "$shim"
done

printf 'Bootstrap complete. Run make check.\n'
