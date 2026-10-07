#!/bin/bash
set -eou pipefail

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
pushd $SCRIPTDIR/.. >/dev/null

TARGET=${DEFAULT_CARGO_TARGET:-$(rustup default | cut -d- -f2- | cut -d' ' -f1)}

if ! command -v zig > /dev/null 2>&1; then
    echo "zig is not installed. Please install 'zig':"
    echo "  macos: brew install zig"
    echo "  other: https://ziglang.org/download/"
    exit 1
fi

if ! command -v cargo-zigbuild > /dev/null 2>&1; then
    echo "cargo-zigbuild is not installed. Install it with: cargo install cargo-zigbuild --locked"
    exit 1
fi

if [ $# -ge 1 ] && [ "$1" = "--cleanup" ] ; then
    echo "Cleanup enabled for clippy."
    CLEANUP=1
    shift
else
    CLEANUP=0
fi

echo "Running clippy for aarch64-apple-darwin..."
cargo-zigbuild clippy --target aarch64-apple-darwin --all-targets --locked $@
if [ $CLEANUP -eq 1 ] ; then
    if [ "$TARGET" != "aarch64-apple-darwin" ] ; then
        rm -rf target/aarch64-apple-darwin
    fi
fi

popd >/dev/null
