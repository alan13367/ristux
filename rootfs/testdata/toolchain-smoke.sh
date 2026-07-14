#!/bin/sh
set -e
export HOME=/root
export PATH=/bin

check_ssh() {
    set +e
    ssh >/tmp/ssh-usage.txt 2>&1
    ssh_status=$?
    set -e
    test "$ssh_status" -eq 255 || exit 1
    echo ssh-client-ok
}

check_local_git() {
    mkdir /tmp/cargo-git-dependency || exit 1
    cd /tmp/cargo-git-dependency
    gzip -dc /usr/share/testdata/cargo-git-dependency.tar.gz | tar -xf - || exit 1
    cp -r /usr/share/testdata/cargo-git-consumer /tmp/cargo-git-consumer || exit 1
    cd /tmp/cargo-git-consumer
    HOME=/root cargo metadata --format-version 1 || exit 1
    echo cargo-local-git-ok
}

rustc --version || exit 1
rustc --print target-list || exit 1
HOME=/root cargo --version || exit 1
rustdoc --version || exit 1
ristux-ld --self-test || exit 1
ristux-ld --self-test-archive || exit 1
HOME=/root rust_host_probe || exit 1
HOME=/root cargo new /tmp/cargo-smoke || exit 1
cat /tmp/cargo-smoke/Cargo.toml
pkg files rust-std-libs
check_ssh
check_local_git
cd /tmp/cargo-smoke
HOME=/root CARGO_INCREMENTAL=0 cargo run || exit 1
echo cargo-smoke-ok
cp -r /usr/share/testdata/cargo-workspace /tmp/cargo-workspace
cd /tmp/cargo-workspace
HOME=/root CARGO_INCREMENTAL=0 cargo run -p workspace-app || exit 1
if [ "$1" = q ]; then
    echo toolchain-smoke: done
    exit 0
fi
HOME=/root rustc_metadata_probe --direct || exit 1
HOME=/root rustc_metadata_probe --hosted || exit 1
pkg info rustc
pkg info cargo
pkg info rustdoc
pkg info rust-host-probe
pkg info rustc-metadata-probe
pkg info rust-core-libs
echo toolchain-smoke: done
