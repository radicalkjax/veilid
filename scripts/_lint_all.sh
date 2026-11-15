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
scripts/_clippy_mock_api.sh $@
scripts/_clippy_optional_features.sh $@

# Flutter analyze for the veilid-flutter pub workspace; one resolution covers every member
echo "==> flutter pub get veilid-flutter"
(cd veilid-flutter && flutter pub get)
echo "==> flutter analyze veilid-flutter"
(cd veilid-flutter && flutter analyze)

popd >/dev/null
