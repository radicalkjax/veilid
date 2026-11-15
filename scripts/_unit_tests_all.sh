#!/bin/bash
# nounset + pipefail; no `set -e` — failures are handled explicitly so this
# script returns the exit code of the first failing test step.
set -uo pipefail

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
pushd "$SCRIPTDIR/.." >/dev/null || exit 1

if ! command -v cargo-nextest > /dev/null 2>&1; then
    echo "cargo-nextest is not installed. Please install it with: cargo install cargo-nextest --locked"
    popd >/dev/null
    exit 1
fi

TARGET=${DEFAULT_CARGO_TARGET:-$(rustup default | cut -d- -f2- | cut -d' ' -f1)}

echo "Running unit tests for target $TARGET..."
status=0
if [ "$TARGET" = "wasm32-unknown-unknown" ]; then
    # Uses cargo nextest but launches with their own chromedriver helper script internally
    echo "Running veilid-tools tests for target $TARGET..."
    ( cd veilid-tools && ./run_tests.sh wasm ) \
        && { echo "Running veilid-core tests for target $TARGET..."; ( cd veilid-core && ./run_tests.sh wasm ); } \
        && { echo "Running veilid-wasm tests for target $TARGET..."; ( cd veilid-wasm && ./wasm_test_js.sh --mock ); }
    status=$?
else
    # The two passes below are separate compilations: resolver-v2 unifies features over the
    # SELECTED packages, so the mock-api pass shares no dependency artifacts with the default one.
    # They are functions purely so the argument lists are not duplicated by the timing below.
    #
    # run_test_attachment_manager, run_test_bootstrap, run_test_resolver_live and
    # txt_records_survive_restart are the native tests that reach the network. The mock pass covers
    # the attachment lifecycle and run_test_resolver covers answer decoding, so all four are
    # excluded to keep this script offline.
    #
    # app-lifecycle is on here, as the app bindings ship it, so its tests run; clippy's
    # `-p veilid-core` pass compiles it off, as veilid-server ships it.
    native_pass() {
        cargo nextest run "$@" --tests --target $TARGET --all-targets --locked --features=debug-locks,veilid-core/app-lifecycle -p veilid-server -p veilid-cli -p veilid-tools -p veilid-core-optimized -p veilid-core -p veilid-remote-api \
            -E 'not (test(=tests::native::run_test_attachment_manager) + test(=tests::native::run_test_bootstrap) + test(=tests::native::run_test_resolver_live) + test(=tests::bootstrap_records::tests::txt_records_survive_restart))'
    }
    # A second pass over veilid-core against the mock, where the DHT and transaction suites run.
    # Additive, not a replacement: mock-api compiles out the subsystem unit tests.
    mock_pass() {
        cargo nextest run "$@" --tests --target $TARGET --all-targets --locked \
            -p veilid-core --no-default-features \
            --features=default-tokio,debug-locks,mock-api
    }

    # Wall time per phase. Each pass is built with --no-run and then run, so the log says
    # separately how long compiling took and how long the tests took, and how the two feature
    # configurations divide -- which is what decides whether splitting this job across two runners
    # is worth a second job's startup cost. The run reuses what --no-run built, so the only cost is
    # one extra nextest invocation.
    timed() {  # timed <label> <function> [args...]
        _label=$1; shift
        _t=$(date +%s)
        "$@"
        _rc=$?
        echo "TIMING $_label $(( $(date +%s) - _t ))s"
        return $_rc
    }

    echo "Skipping run_test_attachment_manager, run_test_bootstrap, run_test_resolver_live and txt_records_survive_restart (need the network)"
    timed native-build native_pass --no-run
    status=$?
    [ $status -eq 0 ] && { timed native-run native_pass; status=$?; }

    if [ $status -eq 0 ]; then
        echo "Running mock-api unit tests for target $TARGET..."
        timed mock-build mock_pass --no-run
        status=$?
        [ $status -eq 0 ] && { timed mock-run mock_pass; status=$?; }
    fi
fi
if [ $status -eq 0 ]; then
    echo "Finished running unit tests for target $TARGET..."
fi

popd >/dev/null
exit $status
