#!/usr/bin/env bash
# Print the Go build tags of the release tailnet helper, from helpers/tailnet/omit.txt.

set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
grep -v -e '^#' -e '^$' "$root/helpers/tailnet/omit.txt" | sed 's/^/ts_omit_/' | paste -sd, -
