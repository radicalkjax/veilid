#!/bin/bash
SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
# pipefail keeps pipe exit codes; no `set -e` — failures are handled explicitly
# so this script returns the test command's exit code.
set -o pipefail

pushd "$SCRIPTDIR" >/dev/null || exit 1
status=0
if [[ "$1" == "wasm" ]]; then
    # shellcheck disable=SC1091
    source "$SCRIPTDIR/../scripts/_chromedriver_helper.sh"
    CHROMEDRIVER="$(chromedriver_binary_path)" || { echo "Failed to resolve chromedriver"; popd >/dev/null; exit 1; }
    export CHROMEDRIVER
    # --test-threads=1: each wasm test spawns its own chromedriver+Chrome; running them in
    # parallel starves Chrome renderer startup ("timed out connecting to Chrome").
    WASM_BINDGEN_TEST_TIMEOUT=120 cargo nextest run --target wasm32-unknown-unknown --no-default-features --features=default-wasm,test-util,debug-locks --test-threads=1
    status=$?
elif [[ "$1" == "ios" ]]; then
    SYMROOT=/tmp/testout
    APPNAME=veilidcore-tests
    # Also the tracing-oslog subsystem, set in src/tests/ios/mod.rs
    BUNDLENAME=com.veilid.veilidcore-tests
    ID="$2"
    if [[ "$ID" == "" ]]; then
        echo "No emulator ID specified"
        popd >/dev/null
        exit 1
    fi

    # Build for simulator
    xcrun xcodebuild -project src/tests/ios/$APPNAME/$APPNAME.xcodeproj/ -scheme $APPNAME -destination "generic/platform=iOS Simulator" SYMROOT=$SYMROOT || { status=$?; popd >/dev/null; exit $status; }

    # Run in temporary simulator
    xcrun simctl install $ID $SYMROOT/Debug-iphonesimulator/$APPNAME.app || { status=$?; popd >/dev/null; exit $status; }
    # Capture os_log output; the completion marker only appears there, not on the console
    IOS_LOG=$(mktemp)
    xcrun simctl spawn $ID log stream --level debug --predicate "subsystem == \"$BUNDLENAME\"" >"$IOS_LOG" 2>&1 &
    LOGPID=$!
    sleep 2 # Let the log stream attach before launching
    xcrun simctl launch --console $ID $BUNDLENAME
    sleep 1 # Ensure the last log lines print
    kill -INT $LOGPID
    # Print the test progression, then assert on the completion marker
    grep -E "TEST:|Finished unit tests|panicked|assertion" "$IOS_LOG" || true
    if grep -q "Finished unit tests" "$IOS_LOG"; then
        echo "iOS unit tests PASSED"
        status=0
    else
        echo "iOS unit tests FAILED or did not complete"
        status=1
    fi
    rm -f "$IOS_LOG"

    # Clean up build output
    rm -rf /tmp/testout

elif [[ "$1" == "android" ]]; then
    ID="$2"
    if [[ "$ID" == "" ]]; then
        echo "No emulator ID specified, trying 'emulator-5554'"
        ID="emulator-5554"
    fi
    APPNAME=veilid_core_android_tests
    APPID=com.veilid.veilid_core_android_tests
    ACTIVITYNAME=MainActivity
    pushd src/tests/android/$APPNAME >/dev/null || { popd >/dev/null; exit 1; }
    # Build apk
    ./gradlew assembleDebug || { status=$?; popd >/dev/null; popd >/dev/null; exit $status; }
    # Wait for boot
    adb -s $ID wait-for-device
    # Install app
    adb -s $ID install -r ./app/build/outputs/apk/debug/app-debug.apk || { status=$?; popd >/dev/null; popd >/dev/null; exit $status; }
    # Clear logcat so we only capture this run
    adb -s $ID logcat -c
    # Start activity
    adb -s $ID shell am start-activity -W $APPID/.$ACTIVITYNAME
    # Wait for the app process to finish (bounded so a hung test fails instead of blocking)
    WAITED=0
    while [ "$(adb -s $ID shell pidof -s $APPID)" != "" ]; do
        sleep 1
        WAITED=$((WAITED + 1))
        if [ $WAITED -ge 300 ]; then
            echo "Timed out waiting for tests to finish"
            break
        fi
    done
    # Print the test output (progression + result), then assert on the completion marker
    ANDROID_LOG="$(adb -s $ID logcat -d)"
    echo "$ANDROID_LOG" | grep -E "TEST:|Starting unit tests|Finished unit tests|panic|FATAL EXCEPTION|AndroidRuntime" || true
    if [[ "$ANDROID_LOG" == *"Finished unit tests"* ]]; then
        echo "Android unit tests PASSED"
        status=0
    else
        echo "Android unit tests FAILED or did not complete"
        status=1
    fi
    # Finished
    popd >/dev/null

else
    echo "Running: RUST_LOG=#common=debug cargo nextest run --features=debug-locks"
    RUST_LOG=#common=debug cargo nextest run --features=debug-locks \
        && echo "Running: RUST_LOG=#common=debug cargo nextest run --no-default-features --features=default-async-std,debug-locks" \
        && RUST_LOG=#common=debug cargo nextest run --no-default-features --features=default-async-std,debug-locks \
        && cargo test --doc
    status=$?
fi
popd >/dev/null
exit $status
