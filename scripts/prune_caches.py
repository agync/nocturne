#!/usr/bin/env python3
"""Print the GitHub Actions caches to delete: all but the newest for each job and platform.

Usage: gh api ... --jq '...' | scripts/prune_caches.py [--now ISO]

Reads `id`, `ref`, `key`, `created_at` and `last_accessed_at`, tab separated, one cache per line,
as the prune workflow asks `gh api` for them. Writes `id` and `key`, tab separated, for each cache
to delete. Only caches on `main` are considered: a pull request's cache is small and belongs to
that pull request. A cache accessed in the last hour is kept, so a prune cannot pull one out from
under a run that is restoring it.

`Swatinem/rust-cache` keys end in a hash of the lock files, after the job and the platform, so
every `Cargo.lock` change starts a new generation of the Rust jobs' caches. Keeping the newest of
each family leaves one generation on the repository, under GitHub's 10 GB, and the previous
generation is still there for the run that restores while a new one is saved.

`setup-go` keeps the Go version in its family, so a Go bump strands the previous cache until GitHub
expires it after 7 days unused.
"""

import re
import sys
from datetime import datetime, timedelta, timezone

MAIN = "refs/heads/main"
GRACE = timedelta(hours=1)

# v<digits>-rust-<key?>-<job>-<os>-<arch>-<environment>-<lockfiles>
RUST = re.compile(r"^(v\d+-rust-.+?-(?:Linux|Darwin|Windows_NT)-(?:x64|arm64))-")
RUST_KEY = re.compile(r"^v\d+-rust-")
# node-cache-<os>-<arch>-npm-<lockfile>, setup-go-<os>-<arch>-<go>-<modules>,
# pre-commit-<os>-<config>, lemonade-<platform>-<lock-and-script>
HASHED = re.compile(r"^(.*)-[0-9a-f]{7,}$")


def family(key):
    """The part of a key that names the job and the platform, without its hashes."""
    match = RUST.match(key)
    if match:
        return match[1]
    if RUST_KEY.match(key):
        raise ValueError(f"unrecognised Rust cache key: {key!r}")
    match = HASHED.match(key)
    return match[1] if match else key


def when(value):
    moment = datetime.fromisoformat(value.replace("Z", "+00:00"))
    return moment if moment.tzinfo else moment.replace(tzinfo=timezone.utc)


def to_delete(rows, now, grace=GRACE):
    """The (id, key) pairs to delete from (id, ref, key, created, accessed) rows."""
    recent = now - grace
    newest = {}
    for cache_id, ref, key, created_at, _ in rows:
        if ref != MAIN:
            continue
        current = newest.get(family(key))
        if current is None or (when(created_at), int(cache_id)) > (
            when(current[0]),
            int(current[1]),
        ):
            newest[family(key)] = (created_at, cache_id, key)
    keep = {cache_id for _, cache_id, _ in newest.values()}
    return [
        (cache_id, key)
        for cache_id, ref, key, _, accessed_at in rows
        if ref == MAIN and cache_id not in keep and when(accessed_at) <= recent
    ]


def main(argv=None):
    args = sys.argv[1:] if argv is None else argv
    now = datetime.now(timezone.utc)
    if args:
        if len(args) == 2 and args[0] == "--now":
            try:
                now = when(args[1])
            except ValueError:
                print(f"--now is not an ISO date: {args[1]!r}", file=sys.stderr)
                return 2
        else:
            print(
                "usage: prune_caches.py [--now ISO] < id, ref, key, created, accessed",
                file=sys.stderr,
            )
            return 2
    rows = []
    for number, line in enumerate(sys.stdin, 1):
        line = line.rstrip("\n")
        if not line:
            continue
        parts = line.split("\t")
        if len(parts) != 5:
            print(f"line {number}: not an id, ref, key and two dates: {line!r}", file=sys.stderr)
            return 1
        rows.append(tuple(parts))
    try:
        for cache_id, key in to_delete(rows, now):
            print(f"{cache_id}\t{key}")
    except ValueError as error:
        print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
