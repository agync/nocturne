#!/usr/bin/env bash
# Build tailscaled and tailscale for macOS and Windows from the pinned upstream tag. Release CI
# runs this; it is not run on a laptop and not by `make check`.
#
# Usage: scripts/build-tailscaled.sh build   <darwin-arm64|windows-amd64> OUT_DIR
#        scripts/build-tailscaled.sh package <darwin-arm64|windows-amd64> OUT_DIR
#        scripts/build-tailscaled.sh licenses OUT_DIR
#
#   build     compiles both programs and writes OUT_DIR/<target>/unsigned/ plus SHA256SUMS. A second,
#             independent build of the same target must print the same sums before anything is
#             pinned; the workflow compares them.
#   package   takes the programs in OUT_DIR/<target>/signed/ (signed and, on macOS, notarised by
#             the workflow) and writes OUT_DIR/tailscaled-<version>-<target>.tgz or .zip with fixed
#             timestamps, then prints the sha256 of the archive and of each program for helpers.lock.
#   licenses  writes OUT_DIR/tailscale-licenses.txt (a go-licenses report: module, licence URL,
#             licence name per line), which scripts/notices.py ships.
#
# The upstream version stamps come from upstream's own build_dist.sh. The build is a static one:
# CGO_ENABLED=0, -trimpath, -buildvcs=false, and the pinned Go toolchain only.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
repo=$PWD
source scripts/sha256.sh

# The Go toolchain every build uses and the commit the pinned tag must point at. Both are
# filled in with the pin (scripts/helpers-pin.sh prints the version; the tag's commit comes from
# `git ls-remote https://github.com/tailscale/tailscale refs/tags/v<version>^{}`).
GO_VERSION=${TAILSCALED_GO_VERSION:-PLACEHOLDER-set-with-the-pin}
TAG_COMMIT=${TAILSCALED_TAG_COMMIT:-PLACEHOLDER-set-with-the-pin}
GO_LICENSES_VERSION=${TAILSCALED_GO_LICENSES_VERSION:-PLACEHOLDER-set-with-the-pin}

version=$(python3 -I -c 'import tomllib; l = tomllib.load(open("helpers.lock", "rb")); print(next(h["version"] for h in l["helpers"] if h["kind"] == "tailscaled"))')
mode=${1:?mode: build, package or licenses}

need_pins() {
    local v
    for v in "$GO_VERSION" "$TAG_COMMIT"; do
        [[ "$v" != PLACEHOLDER* ]] || { printf 'the Go version and the tag commit are not pinned yet\n' >&2; exit 2; }
    done
}

target_vars() {
    case "$1" in
        darwin-arm64) goos=darwin goarch=arm64 exe="" ;;
        windows-amd64) goos=windows goarch=amd64 exe=".exe" ;;
        *) printf 'unknown target: %s\n' "$1" >&2; exit 2 ;;
    esac
}

source_tree() {
    local dir=$1
    if [[ ! -d "$dir/.git" ]]; then
        git clone --quiet --depth 1 --branch "v$version" https://github.com/tailscale/tailscale "$dir"
    fi
    [[ "$(git -C "$dir" rev-parse HEAD)" == "$TAG_COMMIT" ]] \
        || { printf 'tag v%s is not the pinned commit\n' "$version" >&2; exit 1; }
}

case "$mode" in
    build)
        target=${2:?target}
        out=${3:?output folder}
        need_pins
        target_vars "$target"
        [[ "$(go env GOVERSION)" == "go$GO_VERSION" ]] \
            || { printf 'expected go%s, found %s\n' "$GO_VERSION" "$(go env GOVERSION)" >&2; exit 1; }
        work=$(mktemp -d)
        trap 'rm -rf "$work"' EXIT
        source_tree "$work/src"
        cd "$work/src"
        eval "$(./build_dist.sh shellvars)"
        ldflags="-X tailscale.com/version.longStamp=${VERSION_LONG} -X tailscale.com/version.shortStamp=${VERSION_SHORT} -w -s"
        dest="$(cd "$repo" && mkdir -p "$out" && cd "$out" && pwd)/$target/unsigned"
        mkdir -p "$dest"
        for program in tailscaled tailscale; do
            CGO_ENABLED=0 GOOS=$goos GOARCH=$goarch GOFLAGS=-mod=readonly \
                go build -trimpath -buildvcs=false -ldflags "$ldflags" -o "$dest/$program$exe" "./cmd/$program"
        done
        (cd "$dest" && for f in tailscaled$exe tailscale$exe; do printf '%s  %s\n' "$(sha256_of "$f")" "$f"; done) | tee "$dest/SHA256SUMS"
        ;;
    package)
        target=${2:?target}
        out=${3:?output folder}
        target_vars "$target"
        signed="$out/$target/signed"
        [[ -f "$signed/tailscaled$exe" && -f "$signed/tailscale$exe" ]] || { printf 'no signed programs in %s\n' "$signed" >&2; exit 2; }
        day=$(python3 -I -c 'import tomllib; l = tomllib.load(open("helpers.lock", "rb")); print(next(h["pinned_at"] for h in l["helpers"] if h["kind"] == "tailscaled"))')
        archive="$out/tailscaled-$version-$target"
        python3 -I - "$signed" "$archive" "$goos" "$exe" "$day" << 'PY'
import datetime, gzip, io, os, sys, tarfile, zipfile
signed, archive, goos, exe, day = sys.argv[1:6]
stamp = datetime.datetime.fromisoformat(day).replace(tzinfo=datetime.timezone.utc)
names = ["tailscaled" + exe, "tailscale" + exe]
if goos == "windows":
    with zipfile.ZipFile(archive + ".zip", "w", zipfile.ZIP_DEFLATED) as z:
        for name in names:
            info = zipfile.ZipInfo(name, date_time=stamp.timetuple()[:6])
            info.external_attr = 0o755 << 16
            info.compress_type = zipfile.ZIP_DEFLATED
            z.writestr(info, open(os.path.join(signed, name), "rb").read())
else:
    raw = io.BytesIO()
    with tarfile.open(fileobj=raw, mode="w", format=tarfile.GNU_FORMAT) as t:
        for name in names:
            data = open(os.path.join(signed, name), "rb").read()
            info = tarfile.TarInfo(name)
            info.size, info.mode, info.mtime = len(data), 0o755, int(stamp.timestamp())
            info.uid = info.gid = 0
            info.uname = info.gname = ""
            t.addfile(info, io.BytesIO(data))
    with open(archive + ".tgz", "wb") as f:
        with gzip.GzipFile(fileobj=f, mode="wb", mtime=0) as g:
            g.write(raw.getvalue())
PY
        file=$(ls "$archive".tgz "$archive".zip 2> /dev/null | head -1)
        printf 'archive %s\nsha256 %s\n' "$(basename "$file")" "$(sha256_of "$file")"
        printf 'member tailscaled%s  %s\nmember tailscale%s  %s\n' "$exe" "$(sha256_of "$signed/tailscaled$exe")" "$exe" "$(sha256_of "$signed/tailscale$exe")"
        ;;
    licenses)
        out=${2:?output folder}
        need_pins
        [[ "$GO_LICENSES_VERSION" != PLACEHOLDER* ]] || { printf 'go-licenses is not pinned yet\n' >&2; exit 2; }
        work=$(mktemp -d)
        trap 'rm -rf "$work"' EXIT
        source_tree "$work/src"
        mkdir -p "$out"
        GOBIN="$work/bin" go install "github.com/google/go-licenses@$GO_LICENSES_VERSION"
        # One line per module: module, licence URL, licence name.
        (cd "$work/src" && "$work/bin/go-licenses" report ./cmd/tailscaled ./cmd/tailscale 2> /dev/null) \
            > "$out/tailscale-licenses.txt" || { printf 'go-licenses failed\n' >&2; exit 1; }
        printf '%s\n' "$out/tailscale-licenses.txt"
        ;;
    *) printf 'unknown mode: %s\n' "$mode" >&2; exit 2 ;;
esac
