#!/bin/bash
SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
pushd $SCRIPTDIR >/dev/null

CARGO=`which cargo`
CARGO=${CARGO:=~/.cargo/bin/cargo}
CARGO_DIR=$(dirname $CARGO)

CARGO_MANIFEST_PATH=$(python3 -c "import os; import json; print(json.loads(os.popen('$CARGO locate-project').read())['root'])")
CARGO_WORKSPACE_PATH=$(python3 -c "import os; import json; print(json.loads(os.popen('$CARGO locate-project --workspace').read())['root'])")
WORKSPACE_TARGET_PATH=$(python3 -c "import os; print(os.path.realpath(\"$CARGO_WORKSPACE_PATH/../target\"))")
# A relocated cargo target dir (CI caching) moves the lipo inputs with it
TARGET_PATH="${CARGO_TARGET_DIR:-$WORKSPACE_TARGET_PATH}"
PACKAGE_NAME=$1
shift

if [ "$CONFIGURATION" == "Debug" ]; then
    EXTRA_CARGO_OPTIONS="$@ ${VEILID_CARGO_EXTRA_OPTIONS:-}"
    BUILD_MODE="debug"
else
    EXTRA_CARGO_OPTIONS="$@ --release ${VEILID_CARGO_EXTRA_OPTIONS:-}"
    BUILD_MODE="release"
fi
ARCHS=${ARCHS:=arm64}

if [ "$PLATFORM_NAME" == "iphonesimulator" ]; then
    LIPO_OUT_NAME="lipo-ios-sim"
else 
    LIPO_OUT_NAME="lipo-ios"
fi

set -euxo pipefail

# Contained toolchains (dev-setup --prefix / flutter-packer) locate rust via
# RUSTUP_HOME/CARGO_HOME; pass them through the sanitized env when set, or the
# rustup proxy falls back to ~/.rustup — populated on dev Macs, empty on runners.
RUST_ENV=()
[ -n "${RUSTUP_HOME:-}" ] && RUST_ENV+=("RUSTUP_HOME=$RUSTUP_HOME")
[ -n "${CARGO_HOME:-}" ] && RUST_ENV+=("CARGO_HOME=$CARGO_HOME")
[ -n "${CARGO_TARGET_DIR:-}" ] && RUST_ENV+=("CARGO_TARGET_DIR=$CARGO_TARGET_DIR")

LIPOS=""
for arch in $ARCHS
do
    if [ "$arch" == "arm64" ]; then
        echo arm64
        if [ "$PLATFORM_NAME" == "iphonesimulator" ]; then
            CARGO_TARGET=aarch64-apple-ios-sim
        else
            CARGO_TARGET=aarch64-apple-ios
        fi
        CARGO_TOOLCHAIN=
    elif [ "$arch" == "x86_64" ]; then
        echo x86_64
        CARGO_TARGET=x86_64-apple-ios
        CARGO_TOOLCHAIN=
    else
        echo Unsupported ARCH: $arch
        continue
    fi
    
    # Choose arm64 brew for unit tests by default if we are on M1
    if [ -f /opt/homebrew/bin/brew ]; then
        HOMEBREW_DIR=/opt/homebrew/bin
    elif [ -f /usr/local/bin/brew ]; then
        HOMEBREW_DIR=/usr/local/bin
    else 
        HOMEBREW_DIR=$(dirname `which brew`)
    fi

    env -i PATH=/usr/bin:/bin:$HOMEBREW_DIR:$CARGO_DIR HOME="$HOME" USER="$USER" IPHONEOS_DEPLOYMENT_TARGET="$IPHONEOS_DEPLOYMENT_TARGET" ${RUST_ENV[@]+"${RUST_ENV[@]}"} cargo $CARGO_TOOLCHAIN build $EXTRA_CARGO_OPTIONS --target $CARGO_TARGET --manifest-path $CARGO_MANIFEST_PATH

    LIPOS="$LIPOS $TARGET_PATH/$CARGO_TARGET/$BUILD_MODE/lib$PACKAGE_NAME.a"

done

# Make lipo build
mkdir -p "$TARGET_PATH/$LIPO_OUT_NAME/$BUILD_MODE/"
lipo $LIPOS -create -output "$TARGET_PATH/$LIPO_OUT_NAME/$BUILD_MODE/lib$PACKAGE_NAME.a"

# Make most recent dylib available without build mode for flutter
cp "$TARGET_PATH/$LIPO_OUT_NAME/$BUILD_MODE/lib$PACKAGE_NAME.a" "$TARGET_PATH/$LIPO_OUT_NAME/lib$PACKAGE_NAME.a"

# Xcode references the lipo outputs by the static workspace path; when the cargo
# target dir is relocated (CI caching), mirror them back where Xcode looks.
if [ "$TARGET_PATH" != "$WORKSPACE_TARGET_PATH" ]; then
    mkdir -p "$WORKSPACE_TARGET_PATH/$LIPO_OUT_NAME/$BUILD_MODE/"
    cp "$TARGET_PATH/$LIPO_OUT_NAME/$BUILD_MODE/lib$PACKAGE_NAME.a" "$WORKSPACE_TARGET_PATH/$LIPO_OUT_NAME/$BUILD_MODE/"
    cp "$TARGET_PATH/$LIPO_OUT_NAME/lib$PACKAGE_NAME.a" "$WORKSPACE_TARGET_PATH/$LIPO_OUT_NAME/"
fi

popd >/dev/null