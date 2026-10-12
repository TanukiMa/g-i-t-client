#!/bin/sh
# Makes ../giiit_<version>.orig.tar.xz: the source of the working tree plus the vendored crates (needs the
# network once, for `cargo vendor`). debian/ is left out: it belongs to the Debian part of a quilt source package.
# Files that are not tracked and not ignored by git are included, so uncommitted work is in the tarball.
set -eu
cd "$(dirname "$0")/.."
ver=$(sed -n 's/^version = "\(.*\)"/\1/p' src-tauri/Cargo.toml | head -1 | tr -d '\r')
out=${1:-..}
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
dir="$tmp/giiit-$ver"
mkdir "$dir"
git ls-files -z --cached --others --exclude-standard | tar --null --ignore-failed-read -T - -cf - | tar -xf - -C "$dir"
rm -rf "$dir/debian" "$dir/node_modules" "$dir/target" "$dir/src-tauri/gen"
mkdir "$dir/.cargo"
( cd "$dir" && cargo vendor --locked vendor > .cargo/config.toml )
tar -C "$tmp" -cJf "$out/giiit_$ver.orig.tar.xz" "giiit-$ver"
echo "wrote $out/giiit_$ver.orig.tar.xz"
