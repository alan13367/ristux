#!/bin/sh
set -e
export HOME=/root
export PATH=/bin

HOME=/root cargo --version || exit 1

set +e
ssh >/tmp/ssh-usage.txt 2>&1
ssh_status=$?
set -e
test "$ssh_status" -eq 255 || exit 1
echo ssh-client-ok

mkdir /tmp/cargo-git-dependency || exit 1
cd /tmp/cargo-git-dependency
gzip -dc /usr/share/testdata/cargo-git-dependency.tar.gz | tar -xf - || exit 1
cp -r /usr/share/testdata/cargo-git-consumer /tmp/cargo-git-consumer || exit 1
cd /tmp/cargo-git-consumer
echo cargo-local-git-start
git-upload-pack --self-test /tmp/cargo-git-dependency || exit 1
echo cargo-local-git-helper-ok
echo toolchain-smoke: done
