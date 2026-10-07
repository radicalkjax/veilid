#!/bin/bash
set -eou pipefail

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
pushd $SCRIPTDIR/.. >/dev/null

if [ $# -ge 1 ] && [ "$1" = "--cleanup" ] ; then
    CLEANUP="--cleanup"
    shift
else
    CLEANUP=""
fi

# Pin every sub-script's bare cargo/cargo-zigbuild to the CI toolchain so lints match CI.
# Explicit `+nightly` invocations (docs, nightly unit tests) still override this.
RUST_VERSION="$(. "$SCRIPTDIR/../.dagger/versions.env" && echo "$RUST_VERSION")"
if [ -z "$RUST_VERSION" ]; then
    echo "Could not read RUST_VERSION from .dagger/versions.env"
    exit 1
fi
if ! rustup toolchain list | grep -q "^${RUST_VERSION}-"; then
    echo "CI-pinned Rust toolchain $RUST_VERSION is not installed. Install it with:"
    echo "  rustup toolchain install $RUST_VERSION --profile minimal -c clippy -c rustfmt"
    exit 1
fi
export RUSTUP_TOOLCHAIN="$RUST_VERSION"
echo "Using CI-pinned Rust toolchain $RUST_VERSION."

# The sub-scripts cross-compile (cargo-zigbuild) and check for these targets; install any the
# pinned toolchain is missing, else the build fails with "can't find crate for core".
for tgt in aarch64-apple-darwin x86_64-unknown-linux-gnu x86_64-pc-windows-gnu wasm32-unknown-unknown; do
    if ! rustup target list --installed | grep -qx "$tgt"; then
        echo "Installing missing Rust target $tgt for $RUST_VERSION..."
        rustup target add "$tgt"
    fi
done

# docs.rs and the public-api check use the latest rolling nightly; require it up to date so
# nightly-only lint/rustdoc drift is caught here rather than in CI.
NIGHTLY_CHECK="$(rustup check 2>/dev/null | grep '^nightly-' || true)"
if [ -z "$NIGHTLY_CHECK" ]; then
    echo "Rust nightly is not installed (needed for docs and the public-api check). Install it with:"
    echo "  rustup toolchain install nightly --profile minimal"
    exit 1
fi
if echo "$NIGHTLY_CHECK" | grep -qi "update available"; then
    echo "Rust nightly is out of date; CI and docs.rs use the latest nightly. Update it with:"
    echo "  rustup update nightly"
    exit 1
fi

echo "Running all tests..."

# cargo-public-api (nightly rustdoc) and cargo-msrv (per-crate rust-version) select their own
# toolchains; don't force the pin on them.
env -u RUSTUP_TOOLCHAIN ./scripts/_check_public_api.sh
env -u RUSTUP_TOOLCHAIN ./scripts/_msrv_all.sh
./scripts/_lint_all.sh $CLEANUP
./scripts/_smoketest_all.sh $CLEANUP --locked
./scripts/_check_wasm_builds.sh dart # skip cleanup here because WASM is done first in the clippy script and will clean up there
./scripts/_check_wasm_builds.sh js # skip cleanup here because WASM is done first in the clippy script and will clean up there
./scripts/_build_docs.sh $CLEANUP
./scripts/_unit_tests_all.sh
DEFAULT_CARGO_TARGET=wasm32-unknown-unknown ./scripts/_unit_tests_all.sh 

popd >/dev/null