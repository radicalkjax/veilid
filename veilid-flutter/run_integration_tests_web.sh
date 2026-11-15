#!/usr/bin/env bash
# Build the dart-flavored veilid-flutter wasm blob, copy it into the example's
# web/wasm (via example/scripts/wasm_update.sh, the same step a plain
# `flutter run -d chrome` of the example needs), and run its integration tests.
#
# Runs against veilid-core's in-process mock unless --live is given. The
# selection is a cargo feature here rather than the environment the native build
# hook reads, because this blob is built by wasm_build.sh directly - so
# switching costs a wasm rebuild each way.

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
VEILIDDIR="$( cd "$SCRIPTDIR/.." >/dev/null 2>&1 && pwd )"
# pipefail keeps pipe exit codes; no `set -e` — failures are handled explicitly
# so this script returns the flutter-drive test result.
set -o pipefail

# shellcheck source=../scripts/_integration_test_common.sh
source "$VEILIDDIR/scripts/_integration_test_common.sh" || exit 1
# shellcheck source=../scripts/_chromedriver_helper.sh
source "$VEILIDDIR/scripts/_chromedriver_helper.sh" || exit 1

print_help() {
    cat <<EOF
Usage: $0 [options] [-- flutter-drive args...]

Options:
$(print_common_integration_test_options)
  --release-build      Build veilid-flutter wasm in release mode (default: debug)
  --verbose-tracing    Enable veilid-core/verbose-tracing cargo feature (rebuilds wasm)
  --debug              Enable LOG_LEVEL=debug for veilid logs.
                       Also forwards \$LOG_DIRECTIVES if set in the shell env.
  --help, -h           Show this help

Env:
  SKIP_WASM_BUILD=1    Skip the wasm rebuild + copy step (dart-only iteration)
  LOG_DIRECTIVES=...   Forwarded as --dart-define=LOG_DIRECTIVES when set
  VEILID_DEBUG_LOCKS=1 Build with veilid-flutter/debug-locks (off by default:
                       the per-lock-acquire overhead inflates these tests)

This example has no test-group gating; all tests always run.
EOF
}

RELEASE_BUILD=0
VERBOSE_TRACING=0
DEBUG=0
PASSTHROUGH=()

parse_integration_test_flags "$@"
set -- ${INTEGRATION_TEST_ARGS[@]+"${INTEGRATION_TEST_ARGS[@]}"}
while [ $# -gt 0 ]; do
    case "$1" in
        --release-build)   RELEASE_BUILD=1 ;;
        --verbose-tracing) VERBOSE_TRACING=1 ;;
        --debug)           DEBUG=1 ;;
        --help|-h)         print_help; exit 0 ;;
        --)                shift; PASSTHROUGH+=("$@"); break ;;
        *)                 PASSTHROUGH+=("$1") ;;
    esac
    shift
done

WASM_BUILD_ARGS=()
if [ "$RELEASE_BUILD" -eq 1 ]; then
    WASM_BUILD_ARGS+=("release")
fi
if [ "${VEILID_DEBUG_LOCKS:-}" = "1" ]; then
    WASM_BUILD_ARGS+=("--features=veilid-flutter/debug-locks")
fi
if [ "$VERBOSE_TRACING" -eq 1 ]; then
    WASM_BUILD_ARGS+=("--features=veilid-core/verbose-tracing")
fi
if [ "$LIVE" != true ]; then
    # Unions with wasm_build.sh's own set; mock-api-wasm carries default-wasm
    WASM_BUILD_ARGS+=("--features=mock-api-wasm")
fi

DEFINES=()
if [ "$DEBUG" -eq 1 ]; then
    DEFINES+=(--dart-define=LOG_LEVEL=debug)
fi
# Default to the #common=debug log tag group unless LOG_DIRECTIVES is set in the env
DEFINES+=(--dart-define=LOG_DIRECTIVES="${LOG_DIRECTIVES:-#common=debug}")
# Lets a test scale its settle windows down when there is no network to wait for
if [ "$LIVE" != true ]; then
    DEFINES+=(--dart-define=VEILID_MOCK=true)
fi

if [ "${SKIP_WASM_BUILD:-0}" != "1" ]; then
    "$SCRIPTDIR/example/scripts/wasm_update.sh" \
        "${WASM_BUILD_ARGS[@]+${WASM_BUILD_ARGS[@]}}" ||
        { echo "wasm build failed" >&2; exit 1; }
fi

trap stop_chromedriver EXIT
start_chromedriver || { echo "failed to start chromedriver" >&2; exit 1; }
start_chromedriver_log_poller

# flutter drive's own --headless governs the browser chromedriver launches, and
# it defaults to on - so the visible run is the one that has to ask.
HEADLESS_ARG=--no-headless
if [ "$HEADLESS" = true ]; then
    HEADLESS_ARG=--headless
fi

pushd "$SCRIPTDIR/example" >/dev/null || exit 1
# Web Flutter Driver tests require `-d web-server` in `--release` (or --profile):
# debug is not yet supported (flutter/docs Running-Flutter-Driver-tests-with-Web.md).
# `-d chrome` uses a dwds VM-service debug connection that fails on web ("The
# stream `Timer` is not supported on web devices") and hangs; `-d web-server` in
# debug mode silently no-ops the test bodies. In release, web-server drives the
# app through chromedriver's WebDriver bridge and reads results via JS — no VM
# service. Browser console is forwarded by start_chromedriver_log_poller.
run_flutter_drive_for_web_tests \
    --driver=test_driver/integration_test.dart \
    --target=integration_test/app_test.dart \
    -d web-server \
    --browser-name=chrome \
    --release \
    "$HEADLESS_ARG" \
    --verbose \
    "${DEFINES[@]}" \
    ${PASSTHROUGH[@]+"${PASSTHROUGH[@]}"}
status=$?
popd >/dev/null
exit $status
