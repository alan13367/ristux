#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

output="${1:-build/testdata/cargo-git-dependency}"
work="${output}.work"
home="${output}.home"

rm -rf "$output" "$work" "$home"
mkdir -p "$work/src" "$home"
cp rootfs/testdata/cargo-git-dependency/Cargo.toml "$work/Cargo.toml"
cp rootfs/testdata/cargo-git-dependency/src/lib.rs "$work/src/lib.rs"

export HOME="$PWD/$home"
export GIT_CONFIG_NOSYSTEM=1
export GIT_AUTHOR_NAME=Ristux
export GIT_AUTHOR_EMAIL=ristux@example.invalid
export GIT_COMMITTER_NAME="$GIT_AUTHOR_NAME"
export GIT_COMMITTER_EMAIL="$GIT_AUTHOR_EMAIL"
export GIT_AUTHOR_DATE=2000-01-01T00:00:00Z
export GIT_COMMITTER_DATE="$GIT_AUTHOR_DATE"

git -C "$work" init -q --initial-branch=main
git -C "$work" add Cargo.toml src/lib.rs
git -C "$work" commit -qm fixture
commit="$(git -C "$work" rev-parse HEAD)"
git clone -q --bare "$work" "$output"
mkdir -p "$output/refs/heads"
printf '%s\n' "$commit" > "$output/refs/heads/main"
rm -f "$output/packed-refs"
rm -rf "$work" "$home"
