#!/bin/bash
# Shared plumbing for the veilid-flutter integration-test runners: which
# veilid-flutter the suite builds against, and whether the run shows the
# in-window test UI or stays headless.
#
# Source it from a run_integration_tests.sh wrapper:
#   source "${VEILID_ROOT}/scripts/_integration_test_common.sh"
#   parse_integration_test_flags "$@"
#   set -- ${INTEGRATION_TEST_ARGS[@]+"${INTEGRATION_TEST_ARGS[@]}"}
#   select_veilid_build "$LIVE"
#   set_integration_test_device_args "$HEADLESS" "$@" || exit 1
#   flutter test "${INTEGRATION_TEST_DEVICE_ARGS[@]}" ...

###########################################################################
# Which veilid-flutter the tests build against
#
# Mock by default. The suites exercise DHT records, transactions, watches and
# the table store, all of which veilid-core's in-process mock serves with no
# network, and attaching to the real one costs minutes of every run.
#
# The selection is exported into this shell, so every build it launches inherits
# it and nothing survives the run to be picked up by the next one. Hooks run
# under a fixed environment allowlist that passes only a few name prefixes:
# CARGO_ reaches a hook from hooks_runner 1.6.2 on, which no flutter stable
# ships yet, so the CCACHE_ spelling carries the same selection until one does.
# Drop the CCACHE_ names here and in veilid-flutter/hook/build.dart together.

VEILID_BUILD_FEATURE_VARS=(
    CARGO_VEILID_FLUTTER_FEATURES
    CCACHE_VEILID_FLUTTER_FEATURES
)
VEILID_BUILD_NO_DEFAULT_VARS=(
    CARGO_VEILID_FLUTTER_NO_DEFAULT_FEATURES
    CCACHE_VEILID_FLUTTER_NO_DEFAULT_FEATURES
)

# $1  true for the real network, false for the in-process mock
select_veilid_build() {
    local live="$1" var
    # Cleared first either way, so a mock selection exported by the calling
    # shell cannot outlive a --live run
    for var in "${VEILID_BUILD_FEATURE_VARS[@]}" "${VEILID_BUILD_NO_DEFAULT_VARS[@]}"; do
        unset "$var"
    done
    [ "$live" = true ] && return 0

    # mock-api is mutually exclusive with the default `stats` feature, so it has
    # to be the whole set rather than an addition to it
    for var in "${VEILID_BUILD_FEATURE_VARS[@]}"; do
        export "$var=mock-api"
    done
    for var in "${VEILID_BUILD_NO_DEFAULT_VARS[@]}"; do
        export "$var=1"
    done
    return 0
}

###########################################################################
# Common flags

# Strip the flags every runner understands, since flutter would reject them.
# Sets LIVE and HEADLESS, and leaves the rest in INTEGRATION_TEST_ARGS for the
# caller to apply with `set --`.
LIVE=false
HEADLESS=false
INTEGRATION_TEST_ARGS=()
parse_integration_test_flags() {
    local arg
    LIVE=false
    HEADLESS=false
    INTEGRATION_TEST_ARGS=()
    for arg in "$@"; do
        case "$arg" in
            --live)     LIVE=true ;;
            --headless) HEADLESS=true ;;
            *)          INTEGRATION_TEST_ARGS+=("$arg") ;;
        esac
    done
}

# The `--live`/`--headless` block every runner's --help prints, so the wording
# stays identical across repos.
print_common_integration_test_options() {
    cat <<'EOF'
  --live               Run against the real Veilid network instead of the
                       in-process mock (rebuilds the native library)
  --headless           No window: run the suite with no visible UI and put
                       everything on the console. Default is the in-window
                       test UI.
EOF
}

###########################################################################
# Where the suite runs

# The flutter device id for this host's desktop.
integration_test_desktop_device() {
    case "$(uname -s 2>/dev/null)" in
        Darwin)               echo macos ;;
        Linux)                echo linux ;;
        MINGW*|MSYS*|CYGWIN*) echo windows ;;
        *)                    return 1 ;;
    esac
}

# `-d <desktop>` runs the suite as a real app so IntegrationLogApp has a window
# to draw in; `-d flutter-tester` runs the same suite in a headless VM with no
# window at all, which is what --headless and CI want. Naming flutter-tester
# explicitly matters: with no -d at all, a host with more than one device
# attached makes flutter test refuse to pick.
INTEGRATION_TEST_DEVICE_ARGS=()
set_integration_test_device_args() {
    local headless="$1" arg device
    shift

    INTEGRATION_TEST_DEVICE_ARGS=()
    if [ "$headless" = true ]; then
        INTEGRATION_TEST_DEVICE_ARGS=(-d flutter-tester)
        return 0
    fi
    # A -d from the caller wins, e.g. running the suite on a phone
    for arg in "$@"; do
        case "$arg" in
            -d|--device-id|--device-id=*) return 0 ;;
        esac
    done

    device="$(integration_test_desktop_device)" || {
        echo "ERROR: no desktop flutter device for $(uname -s); use --headless." >&2
        return 1
    }
    INTEGRATION_TEST_DEVICE_ARGS=(-d "$device")
    return 0
}

###########################################################################
# Which test groups run

# Every ENABLE_TEST_* gate an app_test.dart declares, one per line. Read out of
# the file so a copy of the list in a runner cannot go stale. Newlines are
# flattened first: the formatter wraps long declarations, and a line-at-a-time
# match silently dropped every gate it split.
integration_test_gates() {
    tr '\n' ' ' < "$1" |
        grep -oE "bool\.fromEnvironment\(\s*'ENABLE_TEST_[A-Z_]+'" |
        grep -oE "ENABLE_TEST_[A-Z_]+" |
        sort -u
}

_integration_test_group_names() {
    printf '%s\n' "$1" | sed 's/^ENABLE_TEST_//' | tr '[:upper:]' '[:lower:]' | tr '\n' ' '
}

# Resolve --tests=a,b (or, with none named, every gate but the excluded ones)
# into the --dart-define=ENABLE_TEST_*=true the suite reads, and leave them in
# INTEGRATION_TEST_GATE_DEFINES. Names are validated against what app_test.dart
# actually declares, so a renamed gate fails loudly instead of silently running
# nothing - a bare run used to report a cheerful "0 tests passed".
#   $1  path to app_test.dart
#   $2  comma-separated group list, empty for all
#   $3+ gates to leave out of an unnamed run
INTEGRATION_TEST_GATE_DEFINES=()
set_integration_test_gate_defines() {
    local app_test="$1" tests="$2" all gate name excluded skip selected
    shift 2

    all="$(integration_test_gates "$app_test")"
    if [ -z "$all" ]; then
        echo "no ENABLE_TEST_* gates found in $app_test" >&2
        return 1
    fi

    INTEGRATION_TEST_GATE_DEFINES=()
    if [ -n "$tests" ]; then
        IFS=',' read -ra selected <<< "$tests"
        for name in ${selected[@]+"${selected[@]}"}; do
            gate="ENABLE_TEST_$(printf '%s' "$name" | tr '[:lower:]' '[:upper:]')"
            if ! printf '%s\n' "$all" | grep -qx "$gate"; then
                echo "Unknown test group: $name" >&2
                echo "Available: $(_integration_test_group_names "$all")" >&2
                return 1
            fi
            INTEGRATION_TEST_GATE_DEFINES+=(--dart-define="$gate=true")
        done
        return 0
    fi

    while read -r gate; do
        skip=false
        for excluded in "$@"; do
            if [ "$gate" = "$excluded" ]; then
                skip=true
                break
            fi
        done
        [ "$skip" = true ] && continue
        INTEGRATION_TEST_GATE_DEFINES+=(--dart-define="$gate=true")
    done <<< "$all"

    echo "Enabling ${#INTEGRATION_TEST_GATE_DEFINES[@]} gates:" >&2
    printf '  %s\n' "${INTEGRATION_TEST_GATE_DEFINES[@]#--dart-define=}" >&2
    return 0
}
