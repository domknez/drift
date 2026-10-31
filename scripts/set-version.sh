#!/bin/sh
# Set drift's version (in Cargo.toml and Cargo.lock) for a release build: the
# release's tag without its "v". The change isn't committed; the repository keeps
# the version it has.
set -e
cd "$(dirname "$0")/.."
version="${1#v}"
case "$version" in
  [0-9]*.[0-9]*.[0-9]*) ;;
  *) echo "usage: $0 <version>, like 1.2.3" >&2; exit 1 ;;
esac
tmp=$(mktemp)
# The package's own version: the first `version =` line in Cargo.toml.
awk -v v="$version" '!done && /^version = / { print "version = \"" v "\""; done = 1; next } { print }' Cargo.toml > "$tmp" && cat "$tmp" > Cargo.toml
# drift's entry in Cargo.lock: the version line after `name = "drift"`.
awk -v v="$version" 'drift && /^version = / { print "version = \"" v "\""; drift = 0; next } { drift = ($0 == "name = \"drift\""); print }' Cargo.lock > "$tmp" && cat "$tmp" > Cargo.lock
rm -f "$tmp"
echo "drift $version"
