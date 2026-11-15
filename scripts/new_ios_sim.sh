#!/bin/bash
set -euo pipefail

DEVICE_TYPE=${1:-com.apple.CoreSimulator.SimDeviceType.iPhone-14-Pro}

# Newest available iOS runtime
RUNTIME=$(xcrun simctl list runtimes available -j \
    | jq -r '[.runtimes[] | select(.isAvailable and (.identifier | contains(".iOS-")))]
        | sort_by(.version | split(".") | map(tonumber)) | last | .identifier')
if [[ -z "$RUNTIME" || "$RUNTIME" == "null" ]]; then
    echo "No available iOS simulator runtime found" >&2
    exit 1
fi
echo "Using runtime $RUNTIME"

if ! ID=$(xcrun simctl create test-iphone "$DEVICE_TYPE" "$RUNTIME"); then
    echo "Failed to create simulator for $DEVICE_TYPE on $RUNTIME" >&2
    xcrun simctl list devicetypes >&2
    exit 1
fi
trap 'xcrun simctl shutdown "$ID" >/dev/null 2>&1 || true; xcrun simctl delete "$ID"' EXIT

xcrun simctl boot "$ID"
xcrun simctl bootstatus "$ID"
echo "Simulator ID is $ID"
( trap exit SIGINT ; read -r -d '' _ </dev/tty ) ## wait for Ctrl-C
