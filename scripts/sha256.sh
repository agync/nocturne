# Source this: portable sha256 helpers. GNU sha256sum on Linux and Windows' Git Bash, shasum on macOS.

if sha256sum --version > /dev/null 2>&1; then
    sha256_tool=(sha256sum)
else
    sha256_tool=(shasum -a 256)
fi

# sha256_of FILE: the hex digest.
sha256_of() { "${sha256_tool[@]}" -- "$1" | cut -d ' ' -f 1; }

# sha256_manifest DIR: "digest  ./path" for every regular file under DIR, in byte order.
sha256_manifest() {
    (cd "$1" && find . -type f -print0 | LC_ALL=C sort -z | xargs -0 "${sha256_tool[@]}")
}
