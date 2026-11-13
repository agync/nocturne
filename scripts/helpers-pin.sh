#!/usr/bin/env bash
# Pin the connection helpers for Away from home. Maintainer-run, never part of `make check`.
#
# Usage: scripts/helpers-pin.sh [--write]
#
# Reads helpers.lock, downloads each pinned download with GET (nothing is sent but the request),
# and prints the size and sha256 of the download and of every binary inside it. With --write the checksums and
# sizes go into helpers.lock and a helper is no longer marked as a placeholder once every platform was
# pinned. Then, for the binary of this machine, it captures the help text of every command
# Nocturne uses, in an empty environment and a network namespace with only loopback
# (`unshare -rn`), into crates/helpers/fixtures/help/<kind>/<version>/<command>.txt.
#
# Assets that do not exist upstream (tailscaled for macOS and Windows) come from Nocturne's own
# release: the tailscaled CI job builds them (scripts/build-tailscaled.sh); pin those after a
# release whose second build reproduced the checksum.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
source scripts/sha256.sh

write=0
case "${1:-}" in
    --write) write=1 ;;
    "") ;;
    *) printf 'usage: scripts/helpers-pin.sh [--write]\n' >&2; exit 2 ;;
esac

scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
results=$scratch/results.tsv
: > "$results"

# kind, version, platform, url, file, member, extras (comma separated members)
python3 -I - << 'PY' > "$scratch/assets.tsv"
import tomllib
with open("helpers.lock", "rb") as handle:
    lock = tomllib.load(handle)
for helper in lock["helpers"]:
    for platform, asset in helper["assets"].items():
        base = asset.get("base_url", helper["base_url"]).rstrip("/")
        print("\t".join([
            helper["kind"], helper["version"], platform, base + "/" + asset["file"], asset["file"],
            asset.get("member", ""), ",".join(e["member"] for e in asset.get("extras", [])),
        ]))
PY

failed=0
while IFS=$'\t' read -r kind version platform url file member extras; do
    dir=$scratch/dl/$kind/$platform
    mkdir -p "$dir"
    printf '%s %s %s\n' "$kind" "$version" "$platform"
    if ! curl --fail --location --silent --show-error -o "$dir/$file" "$url"; then
        printf '  SKIP  could not fetch %s\n' "$url"
        failed=1
        continue
    fi
    archive_sum=$(sha256_of "$dir/$file")
    size=$(stat -c %s "$dir/$file" 2> /dev/null || stat -f %z "$dir/$file")
    member_sum=""
    extra_sums=""
    if [[ -n "$member" ]]; then
        mkdir -p "$dir/x"
        case "$file" in
            *.zip) unzip -q -o "$dir/$file" -d "$dir/x" ;;
            *) tar -xzf "$dir/$file" -C "$dir/x" ;;
        esac
        member_sum=$(sha256_of "$dir/x/$member")
        IFS=, read -ra list <<< "$extras"
        for e in "${list[@]}"; do
            [[ -n "$e" ]] && extra_sums+="${e}=$(sha256_of "$dir/x/$e"),"
        done
    fi
    printf '  sha256 %s\n  size   %s\n' "$archive_sum" "$size"
    [[ -n "$member_sum" ]] && printf '  member %s\n' "$member_sum"
    [[ -n "$extra_sums" ]] && printf '  extras %s\n' "$extra_sums"
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$kind" "$platform" "$archive_sum" "$member_sum" "$extra_sums" "$size" >> "$results"
done < "$scratch/assets.tsv"

if ((write)); then
    python3 -I - "$results" "$failed" << 'PY'
import re, sys
rows = [line.rstrip("\n").split("\t") for line in open(sys.argv[1])]
failed = sys.argv[2] == "1"
text = open("helpers.lock", encoding="utf-8").read()

def block(kind, platform):
    pattern = re.compile(r"(?ms)^(\[\[helpers\]\]\nkind = \"%s\".*?^\[helpers\.assets\.%s\]\n)(.*?)(?=^\[|\Z)" % (kind, platform))
    return pattern

for kind, platform, archive, member, extras, size in rows:
    m = block(kind, platform).search(text)
    if not m:
        continue
    body = m.group(2)
    body = re.sub(r'(?m)^sha256 = ".*"$', 'sha256 = "%s"' % archive, body, count=1)
    body = re.sub(r'(?m)^size = .*$', 'size = %s' % size, body, count=1)
    if member:
        body = re.sub(r'(?m)^member_sha256 = ".*"$', 'member_sha256 = "%s"' % member, body, count=1)
        for pair in filter(None, extras.split(",")):
            name, digest = pair.split("=")
            body = re.sub(r'(\{ member = "%s", sha256 = ")[0-9a-f]+(" \})' % re.escape(name), r"\g<1>%s\g<2>" % digest, body)
    text = text[: m.start(2)] + body + text[m.end(2):]
if not failed:
    text = re.sub(r"(?m)^placeholder = true\n", "", text)
open("helpers.lock", "w", encoding="utf-8").write(text)
print("helpers.lock updated" + ("" if not failed else "; some assets were skipped, so the placeholder marks stay"))
PY
fi

# Help text of the binaries of this machine.
case "$(uname -s)-$(uname -m)" in
    Linux-x86_64) here=linux-x86_64 ;;
    Linux-aarch64) here=linux-aarch64 ;;
    *) printf 'help text is captured on Linux only; skipped\n'; exit "$failed" ;;
esac
command -v unshare > /dev/null && unshare -rn true 2> /dev/null || { printf 'no user and network namespace here (unshare -rn)\n' >&2; exit 2; }

capture() { # kind version outfile program args...
    local kind=$1 version=$2 out=$3
    shift 3
    local target=crates/helpers/fixtures/help/$kind/$version
    mkdir -p "$target"
    mkdir -p "$scratch/home"
    unshare -rn env -i HOME="$scratch/home" PATH= "$@" 2>&1 | sed 's/[[:space:]]*$//' > "$target/$out.txt" || true
    printf 'captured %s/%s\n' "$target" "$out.txt"
}

while IFS=$'\t' read -r kind version platform _; do
    [[ "$platform" == "$here" ]] || continue
    dir=$scratch/dl/$kind/$platform
    [[ -d "$dir" ]] || continue
    case "$kind" in
        cloudflared)
            bin=$(find "$dir" -type f \( -name cloudflared -o -name 'cloudflared-*' \) ! -name '*.tgz' | head -1)
            [[ -n "$bin" ]] || continue
            chmod +x "$bin"
            capture cloudflared "$version" tunnel "$bin" tunnel --help
            ;;
        tailscaled)
            daemon=$(find "$dir/x" -type f -name tailscaled 2> /dev/null | head -1)
            cli=$(find "$dir/x" -type f -name tailscale 2> /dev/null | head -1)
            [[ -n "$daemon" && -n "$cli" ]] || continue
            capture tailscaled "$version" daemon "$daemon" --help
            capture tailscaled "$version" tailscale "$cli" --help
            for c in up set status logout serve; do capture tailscaled "$version" "$c" "$cli" $c --help; done
            for c in status reset set-config; do capture tailscaled "$version" "serve-$c" "$cli" serve $c --help; done
            ;;
    esac
done < "$scratch/assets.tsv"
exit "$failed"
