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

# mock-api swaps out eight subsystems, so it is a second configuration of the same source that no
# other clippy pass covers. It excludes stats, so the feature set has to be spelled out.
echo "Running clippy for x86_64-unknown-linux-gnu with mock-api..."
cargo-zigbuild clippy --target x86_64-unknown-linux-gnu --all-targets --locked \
    -p veilid-core --no-default-features \
    --features=default-tokio,mock-api $@
cargo-zigbuild clippy --target x86_64-unknown-linux-gnu --all-targets --locked \
    -p veilid-server --no-default-features --features=mock-api $@

if [ $CLEANUP -eq 1 ] ; then
    if [ "$TARGET" != "x86_64-unknown-linux-gnu" ] ; then
        rm -rf target/x86_64-unknown-linux-gnu
    fi
fi

popd >/dev/null
