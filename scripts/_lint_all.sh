#!/bin/bash
set -eou pipefail

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
pushd $SCRIPTDIR/.. >/dev/null

# Run one clippy over the workspace with the default host toolchain
# (may differ from CI pinned toolchain, this step does not run in CI)
env -u RUSTUP_TOOLCHAIN scripts/_clippy_host.sh $@

# Rust clippy across all platforms with CI pinned toolchain
scripts/_clippy_wasm32_unknown_unknown.sh $@
scripts/_clippy_x86_64_pc_windows_gnu.sh $@
scripts/_clippy_aarch64_apple_darwin.sh $@
scripts/_clippy_x86_64_unknown_linux_gnu.sh $@
scripts/_clippy_x86_64_unknown_linux_gnu_async_std.sh $@

# Flutter analyze for every dart/flutter package in the workspace
for pkg in \
    veilid-flutter \
    veilid-flutter/example \
    veilid-flutter/packages/veilid_test \
    veilid-flutter/packages/veilid_integration_test ; do
    echo "==> flutter analyze $pkg"
    (cd "$pkg" && flutter analyze)
done

popd >/dev/null
