#!/bin/sh
# Registry tag for each build tool the CI caches, derived from .dagger/versions.env.
#
# The Earthfile, the Dagger module and .gitlab-ci.yml all ask this script instead of
# spelling out tags of their own, so a version bump moves exactly one tool's tag and
# leaves every other cached tool alone.
#
# usage: tool_tags.sh <tool>    print that tool's tag
#        tool_tags.sh --list    print "<tool> <tag>" for every tool
set -eu

VERSIONS_ENV="${VERSIONS_ENV:-$(dirname "$0")/../.dagger/versions.env}"
. "$VERSIONS_ENV"

# Everything except rust, zig, binaryen and flutter is built on top of the rust image; the CI job
# expresses that as two waves with needs:, so this list's order is not load-bearing.
TOOLS="rust cargo-bins astgrep zigbuild zig binaryen flutter"

# Short digest of everything a tag depends on besides its headline version. Alpine and
# ubuntu have sha256sum; macOS only shasum.
key() {
    printf '%s' "$*" | { sha256sum 2>/dev/null || shasum -a 256; } | cut -c1-8
}

# Every tool image is built on one of the base images, so a base bump has to move every tag or
# CI reuses images built on the old one. The tools key on $LINUX_BUILD_IMAGE rather than on the
# deps-base tag deliberately: the base image is what changes a tool binary's linkage, whereas
# DEPS_BASE_REVISION only changes what is installed alongside it. Bumping it does not rebuild the
# tools; bump a tool's own version pin if it needs rebuilding.
tag_for() {
    case "$1" in
        # Toolchain layout, not just the compiler: a target added to versions.env has to
        # land in the cached rustup home or every cross build fails E0463 later.
        rust)       echo "$RUST_VERSION-$(key "$LINUX_BUILD_IMAGE|$RUST_UNIT_TESTS_NIGHTLY_VERSION|$RUST_STABLE_TARGETS|$RUST_NIGHTLY_TARGETS")" ;;
        # Built on top of the rust image, so its tag tracks that too.
        cargo-bins) echo "$RUST_VERSION-$(key "$LINUX_BUILD_IMAGE|$CARGO_MSRV_VERSION|$CARGO_NEXTEST_VERSION|$CARGO_DOCS_RS_VERSION|$CARGO_PUBLIC_API_VERSION|$CARGO_SYNC_README_VERSION")" ;;
        astgrep)    echo "$AST_GREP_VERSION-$(key "$LINUX_BUILD_IMAGE")" ;;
        zigbuild)   echo "$CARGO_ZIGBUILD_VERSION-$(key "$LINUX_BUILD_IMAGE")" ;;
        zig)        echo "$ZIG_VERSION-$(key "$LINUX_BUILD_IMAGE")" ;;
        binaryen)   echo "$BINARYEN_VERSION-$(key "$LINUX_BUILD_IMAGE")" ;;
        flutter)    echo "$FLUTTER_VERSION-$(key "$FLUTTER_BUILD_IMAGE")" ;;
        # The assembled toolchain image (deps-base + every linux tool merged in). Deliberately
        # not in $TOOLS: it is not a tool, and --list drives the per-tool CI build matrix. Its
        # key is every input that goes into it, so one tool bump moves it and nothing else does.
        # The OS layer: base image and apt packages. The heaviest thing in the stack and
        # the least frequently changed, so it is published once and pulled by every tool build.
        deps-base)  echo "$(key "$LINUX_BUILD_IMAGE|$DEPS_BASE_REVISION")" ;;
        deps-linux) echo "$(key "$(tag_for deps-base)|$(for t in rust cargo-bins astgrep zigbuild zig binaryen; do tag_for "$t"; done)")" ;;
        # The flutter gates' own base: FLUTTER_BUILD_IMAGE + its apt prerequisites + the SDK.
        deps-flutter) echo "$(key "$FLUTTER_BUILD_IMAGE|$DEPS_FLUTTER_REVISION|$(tag_for flutter)")" ;;
        # ...plus the rust toolchain, for the integration gate that builds the native library.
        deps-flutter-rust) echo "$(key "$(tag_for deps-flutter)|$(tag_for rust)")" ;;
        # The cross toolchain the prebuilt veilid-flutter Windows library is built with.
        deps-xwin) echo "$(key "$(tag_for deps-base)|$(tag_for rust)|$XWIN_VERSION|$XWIN_CRT_VERSION|$XWIN_SDK_VERSION")" ;;
        *)          echo "tool_tags: unknown tool '$1' (have: $TOOLS deps-base deps-linux deps-flutter deps-flutter-rust deps-xwin)" >&2; return 1 ;;
    esac
}

if [ "${1:-}" = "--list" ]; then
    for tool in $TOOLS; do
        echo "$tool $(tag_for "$tool")"
    done
else
    tag_for "${1:?usage: tool_tags.sh <tool>|--list}"
fi
