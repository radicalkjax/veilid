#!/bin/bash
# Run the veilid-flutter example integration tests on this host's desktop.
#
# Usage:
#   ./run_integration_tests.sh                 # mock, in-window test UI
#   ./run_integration_tests.sh --headless      # mock, console only (CI)
#   ./run_integration_tests.sh --live          # against the real network
#
# Runs against veilid-core's in-process mock unless --live is given; see
# ../scripts/_integration_test_common.sh. Switching between the two rebuilds the
# native library, so alternating costs a cargo build each way.

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
VEILIDDIR="$( cd "$SCRIPTDIR/.." >/dev/null 2>&1 && pwd )"
# shellcheck source=../scripts/_integration_test_common.sh
source "$VEILIDDIR/scripts/_integration_test_common.sh" || exit 1

print_help() {
    cat <<EOF
Usage: $0 [options] [-- flutter-test args...]

Options:
$(print_common_integration_test_options)
  --help, -h           Show this help

Env:
  LOG_DIRECTIVES=...   Forwarded as --dart-define=LOG_DIRECTIVES (default '#common=debug')

Extra cargo features go in veilid-flutter/pubspec.yaml (the workspace root) under
\`hooks: user_defines:\`, which apply to a --live run. This script owns the feature
set for a mock run.
EOF
}

case " $* " in
    *" --help "*|*" -h "*) print_help; exit 0 ;;
esac

parse_integration_test_flags "$@"
set -- ${INTEGRATION_TEST_ARGS[@]+"${INTEGRATION_TEST_ARGS[@]}"}
select_veilid_build "$LIVE"
set_integration_test_device_args "$HEADLESS" "$@" || exit 1

# Default to the #common=debug log tag group unless the caller overrides LOG_DIRECTIVES
case "$*" in
    *LOG_DIRECTIVES*) ;;
    *) set -- --dart-define=LOG_DIRECTIVES="${LOG_DIRECTIVES:-#common=debug}" "$@" ;;
esac

# Lets a test scale its settle windows down when there is no network to wait for
if [ "$LIVE" != true ]; then
    set -- --dart-define=VEILID_MOCK=true "$@"
fi

pushd "$SCRIPTDIR/example" >/dev/null || exit 1
flutter test -r github \
    ${INTEGRATION_TEST_DEVICE_ARGS[@]+"${INTEGRATION_TEST_DEVICE_ARGS[@]}"} \
    integration_test/app_test.dart "$@"
status=$?
popd >/dev/null
exit $status
