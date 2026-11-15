#!/bin/bash
set -uo pipefail

UNAME_M=${1-$(uname -m)}
if [[ "$UNAME_M" == "arm64" ]]; then
    ANDROID_ABI=arm64-v8a
elif [[ "$UNAME_M" == "x86_64" ]]; then
    ANDROID_ABI=x86
else
    echo "Unknown platform"
    exit 1
fi
AVD_NAME="testavd"
AVD_TAG="google_atd"
AVD_IMAGE="system-images;android-30;$AVD_TAG;$ANDROID_ABI"
AVD_DEVICE="Nexus 10"

if [[ -z "${ANDROID_HOME:-}" ]]; then
    echo "ANDROID_HOME is not set"
    exit 1
fi
SDKMANAGER=$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager
AVDMANAGER=$ANDROID_HOME/cmdline-tools/latest/bin/avdmanager
if ! command -v "$SDKMANAGER" >/dev/null; then
    echo "Can't find 'sdkmanager' in the usual places."
    exit 1
fi

EMULATOR=$ANDROID_HOME/emulator/emulator
ADB=$ANDROID_HOME/platform-tools/adb
if ! command -v "$EMULATOR" >/dev/null; then
    echo "Can't find 'emulator' in the usual places."
    exit 1
fi

# Install AVD image
"$SDKMANAGER" --install "$AVD_IMAGE" || exit 1
# Make AVD
echo "no" | "$AVDMANAGER" --verbose create avd --force --name "$AVD_NAME" --package "$AVD_IMAGE" --tag "$AVD_TAG" --abi "$ANDROID_ABI" --device "$AVD_DEVICE" || exit 1
# Run emulator
"$EMULATOR" -avd "$AVD_NAME" -no-snapshot -no-boot-anim -no-window &
EMULATOR_PID=$!
trap 'kill $EMULATOR_PID 2>/dev/null; wait' EXIT

# Wait for full boot so run_tests.sh can install immediately
"$ADB" wait-for-device
while [[ "$("$ADB" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" != "1" ]]; do
    sleep 1
done
echo "Emulator is booted and ready"
( trap exit SIGINT ; read -r -d '' _ </dev/tty ) ## wait for Ctrl-C
