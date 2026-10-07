#!/bin/bash

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
pushd $SCRIPTDIR >/dev/null

# Fail out if any step has an error
set -e

if [ "$1" == "patch" ]; then
    echo Bumping patch version
    PART=patch
elif [ "$1" == "minor" ]; then
    echo Bumping minor version
    PART=minor
elif [ "$1" == "major" ]; then
    echo Bumping major version
    PART=major
else
    echo Unsupported part! Specify 'patch', 'minor', or 'major'
    exit 1
fi

# Ensure bump-my-version is installed as a uv tool
if ! uv tool run bump-my-version --version >/dev/null 2>&1; then
    echo Installing bump-my-version
    uv tool install bump-my-version
fi

# Change version of crates and packages everywhere
uv tool run bump-my-version bump $PART

# Get the new version we bumped to
NEW_VERSION=$(uv tool run bump-my-version show current_version 2>/dev/null)
echo NEW_VERSION=$NEW_VERSION

# Update crate dependencies for the crates we publish
cargo upgrade -p veilid-tools@$NEW_VERSION -p veilid-core@$NEW_VERSION -p veilid-remote-api@$NEW_VERSION

# Update lockfile
cargo update -w

# Update python lockfile
pushd veilid-python 2>/dev/null
uv lock
popd 2>/dev/null

# Update flutter lockfiles
pushd veilid-flutter 2>/dev/null
flutter pub get
popd 2>/dev/null

# Update flutter package lockfiles
pushd veilid-flutter/packages/veilid_test 2>/dev/null
flutter pub get
popd 2>/dev/null

pushd veilid-flutter/packages/veilid_integration_test 2>/dev/null
flutter pub get
popd 2>/dev/null

# Build wasm bundle (debug) — refreshes pkg/package.json with new version
# so the local tests/ dependency resolves to the bumped version.
pushd veilid-wasm 2>/dev/null
./wasm_build_js.sh
popd 2>/dev/null

pushd veilid-wasm/tests 2>/dev/null
npm install --package-lock-only
popd 2>/dev/null

# Run smoketests without locking dependencies to update lockfiles
scripts/_smoketest_all.sh

popd >/dev/null