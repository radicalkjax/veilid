VERSION 0.8

########################################################################################################################
## ARGUMENTS
##
## CI_REGISTRY_IMAGE - used so that forks can refer to themselves, e.g. to pull the fork's own crate cache in the
## `+fetch-cache` target, and defaulting to `registry.gitlab.com/veilid/veilid` if not specified
##
## CRATE_CACHE_TAG - when set, `+code-linux` starts from `crate-cache:$CRATE_CACHE_TAG` in the project's Container
## Registry, which carries every crate in Cargo.lock pre-downloaded. Unset means fetch them during the build.
##
########################################################################################################################

# Ensure we are using an amd64 platform because some of these targets use cross-platform tooling.
# The binaries' portability comes from GLIBC_FLOOR (cargo-zigbuild pins it), not from this image.
# Must match LINUX_BUILD_IMAGE in .dagger/versions.env: this FROM runs before that file has been
# copied in, so the literal is duplicated here and check_all.sh fails if the two drift.
ARG --global LINUX_BUILD_IMAGE=ubuntu:24.04
FROM $LINUX_BUILD_IMAGE
# Tool versions come from the single source of truth, .dagger/versions.env (the
# dev-setup scripts source the same file). Read here as global ARGs via command
# substitution so every RUN/ENV below uses them unchanged.
COPY .dagger/versions.env /etc/veilid-versions.env
COPY scripts/tool_tags.sh /etc/veilid-tool-tags.sh
COPY scripts/build_jobs.sh /etc/veilid-build-jobs.sh
ARG --global ZIG_VERSION=$(. /etc/veilid-versions.env && echo $ZIG_VERSION)
ARG --global BINARYEN_VERSION=$(. /etc/veilid-versions.env && echo $BINARYEN_VERSION)
ARG --global RUST_VERSION=$(. /etc/veilid-versions.env && echo $RUST_VERSION)
ARG --global RUST_UNIT_TESTS_NIGHTLY_VERSION=$(. /etc/veilid-versions.env && echo $RUST_UNIT_TESTS_NIGHTLY_VERSION)
ARG --global RUST_PACKAGE_TESTS_NIGHTLY_VERSION=$(. /etc/veilid-versions.env && echo $RUST_PACKAGE_TESTS_NIGHTLY_VERSION)
ARG --global AST_GREP_VERSION=$(. /etc/veilid-versions.env && echo $AST_GREP_VERSION)
ARG --global GLIBC_FLOOR=$(. /etc/veilid-versions.env && echo $GLIBC_FLOOR)
ARG --global SELINUX_POLICY_IMAGE=$(. /etc/veilid-versions.env && echo $SELINUX_POLICY_IMAGE)
ARG --global CARGO_ZIGBUILD_VERSION=$(. /etc/veilid-versions.env && echo $CARGO_ZIGBUILD_VERSION)
ARG --global CARGO_MSRV_VERSION=$(. /etc/veilid-versions.env && echo $CARGO_MSRV_VERSION)
ARG --global CARGO_NEXTEST_VERSION=$(. /etc/veilid-versions.env && echo $CARGO_NEXTEST_VERSION)
ARG --global CARGO_DOCS_RS_VERSION=$(. /etc/veilid-versions.env && echo $CARGO_DOCS_RS_VERSION)
ARG --global CARGO_PUBLIC_API_VERSION=$(. /etc/veilid-versions.env && echo $CARGO_PUBLIC_API_VERSION)
ARG --global CARGO_SYNC_README_VERSION=$(. /etc/veilid-versions.env && echo $CARGO_SYNC_README_VERSION)
ARG --global NDK_VERSION=$(. /etc/veilid-versions.env && echo $ANDROID_NDK_VERSION)
ARG --global ANDROID_VERSION=$(. /etc/veilid-versions.env && echo $ANDROID_PLATFORM_VERSION)
ARG --global ANDROID_BUILD_TOOLS_VERSION=$(. /etc/veilid-versions.env && echo $ANDROID_BUILD_TOOLS_VERSION)
ARG --global ANDROID_CMDLINE_TOOLS_VERSION=$(. /etc/veilid-versions.env && echo $ANDROID_CMDLINE_TOOLS_VERSION)
ARG --global ANDROID_CMAKE_VERSION=$(. /etc/veilid-versions.env && echo $ANDROID_CMAKE_VERSION)
ARG --global RUST_STABLE_TARGETS=$(. /etc/veilid-versions.env && echo $RUST_STABLE_TARGETS)
ARG --global RUST_NIGHTLY_TARGETS=$(. /etc/veilid-versions.env && echo $RUST_NIGHTLY_TARGETS)
ARG --global FLUTTER_BUILD_IMAGE=$(. /etc/veilid-versions.env && echo $FLUTTER_BUILD_IMAGE)
ARG --global NODE_MAJOR=$(. /etc/veilid-versions.env && echo $NODE_MAJOR)
ARG --global XWIN_VERSION=$(. /etc/veilid-versions.env && echo $XWIN_VERSION)
ARG --global XWIN_CRT_VERSION=$(. /etc/veilid-versions.env && echo $XWIN_CRT_VERSION)
ARG --global XWIN_SDK_VERSION=$(. /etc/veilid-versions.env && echo $XWIN_SDK_VERSION)
# Where +tool-* comes from: `local` builds it here, `container` pulls the per-version image
# the build_tools CI job pushed. Tags come from the same script .gitlab-ci.yml asks, so a
# version bump in versions.env moves one tool's tag and leaves the rest of the cache alone.
ARG --global TOOLS=local
ARG --global TOOLS_IMAGE=registry.gitlab.com/veilid/veilid
ARG --global TOOL_TAG_RUST=$(VERSIONS_ENV=/etc/veilid-versions.env sh /etc/veilid-tool-tags.sh rust)
ARG --global TOOL_TAG_CARGO_BINS=$(VERSIONS_ENV=/etc/veilid-versions.env sh /etc/veilid-tool-tags.sh cargo-bins)
ARG --global TOOL_TAG_ASTGREP=$(VERSIONS_ENV=/etc/veilid-versions.env sh /etc/veilid-tool-tags.sh astgrep)
ARG --global TOOL_TAG_ZIGBUILD=$(VERSIONS_ENV=/etc/veilid-versions.env sh /etc/veilid-tool-tags.sh zigbuild)
ARG --global TOOL_TAG_ZIG=$(VERSIONS_ENV=/etc/veilid-versions.env sh /etc/veilid-tool-tags.sh zig)
ARG --global TOOL_TAG_BINARYEN=$(VERSIONS_ENV=/etc/veilid-versions.env sh /etc/veilid-tool-tags.sh binaryen)
ARG --global DEPS_BASE_TAG=$(VERSIONS_ENV=/etc/veilid-versions.env sh /etc/veilid-tool-tags.sh deps-base)
ARG --global DEPS_LINUX_TAG=$(VERSIONS_ENV=/etc/veilid-versions.env sh /etc/veilid-tool-tags.sh deps-linux)
ARG --global DEPS_FLUTTER_TAG=$(VERSIONS_ENV=/etc/veilid-versions.env sh /etc/veilid-tool-tags.sh deps-flutter)
ARG --global DEPS_FLUTTER_RUST_TAG=$(VERSIONS_ENV=/etc/veilid-versions.env sh /etc/veilid-tool-tags.sh deps-flutter-rust)
ARG --global DEPS_XWIN_TAG=$(VERSIONS_ENV=/etc/veilid-versions.env sh /etc/veilid-tool-tags.sh deps-xwin)
ARG --global TOOL_TAG_FLUTTER=$(VERSIONS_ENV=/etc/veilid-versions.env sh /etc/veilid-tool-tags.sh flutter)
ENV BINSTALL_DISABLE_TELEMETRY=true
ENV BINSTALL_NO_CONFIRM=true
ENV ANDROID_COMMAND_LINE_TOOLS=https://dl.google.com/android/repository/commandlinetools-linux-${ANDROID_CMDLINE_TOOLS_VERSION}_latest.zip
ENV RUSTUP_HOME=/usr/local/rustup
ENV RUSTUP_DIST_SERVER=https://static.rust-lang.org
ENV CARGO_HOME=/usr/local/cargo
ENV PATH=$PATH:/usr/local/cargo/bin:/usr/local/zig
ENV LD_LIBRARY_PATH=/usr/local/lib
ENV RUST_BACKTRACE=1
ENV RETRY_COUNT=12
# Disable incremental compilation in CI - useless in ephemeral containers and wastes disk
ENV CARGO_INCREMENTAL=0 
# Reduce debug info to line-tables-only (still gives backtraces, much smaller binaries)
ENV CARGO_PROFILE_TEST_DEBUGINFO=1
# Scaled to the machine the build lands on rather than pinned: every rust job here is
# compile-bound (test_unit_tests_all measured 732s compiling against 37s running tests), so this
# is what lets a larger runner actually pay for itself. See scripts/build_jobs.sh for the rule.
# This was hardcoded to 1 under a comment describing "medium (2 vCPU, 8GB)" -- those are in fact
# saas-linux-SMALL's specs; medium is 4 vCPU/16GB, so three of four cores sat idle.
ARG --global CARGO_JOBS=$(sh /etc/veilid-build-jobs.sh)
ENV CARGO_BUILD_JOBS=$CARGO_JOBS
    

WORKDIR /veilid

IF [ $(arch) = "x86_64" ]
    ENV DEFAULT_CARGO_TARGET = "x86_64-unknown-linux-gnu"
    ENV DEFAULT_CARGO_MUSL_TARGET = "x86_64-unknown-linux-musl"
ELSE IF [ $(arch) = "aarch64" ]
    ENV DEFAULT_CARGO_TARGET = "aarch64-unknown-linux-gnu"
    ENV DEFAULT_CARGO_MUSL_TARGET = "aarch64-unknown-linux-musl"
ELSE
    RUN echo "Unsupported host platform"
    RUN false
END

# Install build prerequisites & setup required directories.
# Cached as its own registry image: this is the heaviest and least frequently changed layer in
# the stack, and without it every parallel tool job rebuilds the whole apt install, and
# every image layered above re-uploads it instead of deduping against blobs already in the registry.
deps-base-build:
    RUN echo '\
        Acquire::Retries "'$RETRY_COUNT'";\
        Acquire::https::Timeout "240";\
        Acquire::http::Timeout "240";\
        APT::Get::Assume-Yes "true";\
        APT::Install-Recommends "false";\
        APT::Install-Suggests "false";\
        Debug::Acquire::https "true";\
        ' > /etc/apt/apt.conf.d/99custom
    # Retry wrapper for flaky network steps. curl's own --retry does not cover these: it
    # ignores TLS handshake failures, and for `curl | sh` it only guards fetching the script.
    RUN printf '%s\n' \
        '#!/bin/sh' \
        'i=1' \
        'while ! "$@"; do' \
        '    if [ "$i" -ge "${RETRY_COUNT:-12}" ]; then echo "giving up after $i attempts: $*" >&2; exit 1; fi' \
        '    echo "retry #$i: $*" >&2' \
        '    i=$((i+1))' \
        '    sleep 10' \
        'done' \
        > /usr/local/bin/retry && chmod +x /usr/local/bin/retry
    RUN apt-get -y update
    RUN apt-get install -y ca-certificates iproute2 curl gnupg build-essential libssl-dev openssl file git pkg-config libdbus-1-dev libdbus-glib-1-dev libgirepository1.0-dev libcairo2-dev checkinstall unzip libncursesw5-dev libncurses5-dev jq selinux-policy-dev bzip2
    # Node for the wasm test harness lint (scripts/_lint_typescript.sh): the NodeSource major pinned
    # in versions.env, since the distribution's nodejs is too old for eslint 9
    RUN retry sh -c "curl -fsSL https://deb.nodesource.com/setup_$NODE_MAJOR.x | bash -" && apt-get install -y nodejs
    IF [ $(arch) = "x86_64" ]
        RUN apt-get install -y gcc-aarch64-linux-gnu
    ELSE IF [ $(arch) = "aarch64" ]
        RUN apt-get install -y gcc-x86-64-linux-gnu
    ELSE
        RUN apt-get install -y gcc-aarch64-linux-gnu gcc-x86-64-linux-gnu
    END

    SAVE IMAGE --push $TOOLS_IMAGE/deps-base:$DEPS_BASE_TAG

deps-base:
    IF [ "$TOOLS" = "container" ]
        FROM $TOOLS_IMAGE/deps-base:$DEPS_BASE_TAG
    ELSE
        FROM +deps-base-build
    END
    # Re-assert after the FROM: a published image carries the ENV it was BUILT with, and starting
    # FROM an external image replaces the base recipe's environment entirely. Without this the
    # autodetected job count is frozen at whatever the cache image was baked with -- which is
    # exactly what happened: crate-cache carried CARGO_BUILD_JOBS=1 and every test job compiled
    # single-threaded no matter how large the runner was.
    ENV CARGO_BUILD_JOBS=$CARGO_JOBS

########################################################################################################################
## CACHED BUILD TOOLS
##
## Each tool is fetched or compiled once per version and kept as its own registry image, tagged by
## scripts/tool_tags.sh. `+tool-<name>` hands over an artifact from either that image (TOOLS=container,
## what CI passes) or a local build (TOOLS=local, the default when a dev runs earthly by hand). Only
## the `-build` half carries SAVE IMAGE, so nothing is pushed unless build_tools asks for it by name.
########################################################################################################################

# Rust toolchain: the pinned compiler, clippy, every target in versions.env, and the test nightly
tool-rust-build:
    FROM +deps-base
    RUN curl --retry $RETRY_COUNT --retry-connrefused --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --default-toolchain=$RUST_VERSION -y -c clippy --no-modify-path --profile minimal
    RUN chmod -R a+w $RUSTUP_HOME $CARGO_HOME; \
        rustup --version; \
        cargo --version; \
        rustc --version;
    # Targets come from versions.env (RUST_STABLE_TARGETS); unquoted so the shell word-splits.
    RUN retry rustup target add $RUST_STABLE_TARGETS
    RUN retry rustup target add --toolchain nightly $RUST_NIGHTLY_TARGETS
    RUN retry rustup toolchain install $RUST_UNIT_TESTS_NIGHTLY_VERSION -c miri,rust-src
    SAVE IMAGE --push $TOOLS_IMAGE/tool-rust:$TOOL_TAG_RUST

tool-rust:
    IF [ "$TOOLS" = "container" ]
        FROM $TOOLS_IMAGE/tool-rust:$TOOL_TAG_RUST
    ELSE
        FROM +tool-rust-build
    END
    # Literal paths, not $RUSTUP_HOME/$CARGO_HOME: under TOOLS=container the FROM above is a
    # pulled image, and earthly expands artifact paths from what the Earthfile declares, not
    # from the env baked into someone else's image.
    SAVE ARTIFACT /usr/local/rustup rustup
    SAVE ARTIFACT /usr/local/cargo cargo

# Cargo helpers. Installed under /tools with --root so the artifact is just these binaries and
# not the whole cargo home, which lets a version bump here leave the rust image alone.
tool-cargo-bins-build:
    FROM +tool-rust
    RUN mkdir -p /tools/bin
    # cargo-binstall itself, which +code-linux also needs at build time for wasm-bindgen-cli
    RUN retry bash -c "set -o pipefail; curl --retry $RETRY_COUNT --retry-connrefused -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash"
    RUN cp $CARGO_HOME/bin/cargo-binstall /tools/bin/
    # Binstall the musl build first, fall back to a source build
    # (Doing it this way so we don't have to install the musl rust targets, which binstall will default to building with)
    # MSRV checking tool
    RUN cargo binstall --root /tools --version $CARGO_MSRV_VERSION cargo-msrv --disable-strategies=compile --targets $DEFAULT_CARGO_MUSL_TARGET || \
        cargo install --root /tools --version $CARGO_MSRV_VERSION cargo-msrv --locked
    # Nextest tool
    RUN cargo binstall --root /tools --version $CARGO_NEXTEST_VERSION cargo-nextest --disable-strategies=compile --targets $DEFAULT_CARGO_MUSL_TARGET || \
        cargo install --root /tools --version $CARGO_NEXTEST_VERSION cargo-nextest --locked
    # Docs-rs tool
    RUN cargo binstall --root /tools --version $CARGO_DOCS_RS_VERSION cargo-docs-rs --disable-strategies=compile --targets $DEFAULT_CARGO_MUSL_TARGET || \
        cargo install --root /tools --version $CARGO_DOCS_RS_VERSION cargo-docs-rs --locked
    # Public API checking tool
    RUN cargo binstall --root /tools --version $CARGO_PUBLIC_API_VERSION cargo-public-api --disable-strategies=compile --targets $DEFAULT_CARGO_MUSL_TARGET || \
        cargo install --root /tools --version $CARGO_PUBLIC_API_VERSION cargo-public-api --locked
    # README sync checking tool
    RUN cargo binstall --root /tools --version $CARGO_SYNC_README_VERSION cargo-sync-readme --disable-strategies=compile --targets $DEFAULT_CARGO_MUSL_TARGET || \
        cargo install --root /tools --version $CARGO_SYNC_README_VERSION cargo-sync-readme --locked
    SAVE IMAGE --push $TOOLS_IMAGE/tool-cargo-bins:$TOOL_TAG_CARGO_BINS

tool-cargo-bins:
    IF [ "$TOOLS" = "container" ]
        FROM $TOOLS_IMAGE/tool-cargo-bins:$TOOL_TAG_CARGO_BINS
    ELSE
        FROM +tool-cargo-bins-build
    END
    SAVE ARTIFACT /tools/bin bin

# Census gate engine. Built from source deliberately: ast-grep ships no musl artifact and
# its linux-gnu release targets a newer glibc than this base image, so binstall would
# fetch a binary that cannot run here.
tool-astgrep-build:
    FROM +tool-rust
    RUN cargo install --root /tools --version $AST_GREP_VERSION ast-grep --locked
    SAVE IMAGE --push $TOOLS_IMAGE/tool-astgrep:$TOOL_TAG_ASTGREP

tool-astgrep:
    IF [ "$TOOLS" = "container" ]
        FROM $TOOLS_IMAGE/tool-astgrep:$TOOL_TAG_ASTGREP
    ELSE
        FROM +tool-astgrep-build
    END
    SAVE ARTIFACT /tools/bin bin

# Linux cross-platform tooling
tool-zig-build:
    FROM +deps-base
    RUN retry curl --retry $RETRY_COUNT --retry-connrefused -L -O https://ziglang.org/download/$ZIG_VERSION/zig-linux-$(arch)-$ZIG_VERSION.tar.xz
    RUN tar -C /usr/local -xJf zig-linux-$(arch)-$ZIG_VERSION.tar.xz
    RUN mv /usr/local/zig-linux-$(arch)-$ZIG_VERSION /usr/local/zig
    SAVE IMAGE --push $TOOLS_IMAGE/tool-zig:$TOOL_TAG_ZIG

tool-zig:
    IF [ "$TOOLS" = "container" ]
        FROM $TOOLS_IMAGE/tool-zig:$TOOL_TAG_ZIG
    ELSE
        FROM +tool-zig-build
    END
    SAVE ARTIFACT /usr/local/zig zig

# No binstall available for zigbuild yet, so this one is a source build
tool-zigbuild-build:
    FROM +tool-rust
    RUN cargo install --root /tools --version $CARGO_ZIGBUILD_VERSION cargo-zigbuild --locked
    SAVE IMAGE --push $TOOLS_IMAGE/tool-zigbuild:$TOOL_TAG_ZIGBUILD

tool-zigbuild:
    IF [ "$TOOLS" = "container" ]
        FROM $TOOLS_IMAGE/tool-zigbuild:$TOOL_TAG_ZIGBUILD
    ELSE
        FROM +tool-zigbuild-build
    END
    SAVE ARTIFACT /tools/bin bin

# wasm-opt
tool-binaryen-build:
    FROM +deps-base
    RUN retry curl --retry $RETRY_COUNT --retry-connrefused -L -O https://github.com/WebAssembly/binaryen/releases/download/version_$BINARYEN_VERSION/binaryen-version_$BINARYEN_VERSION-$(arch)-linux.tar.gz
    RUN mkdir /tmp/binaryen
    RUN tar -C /tmp/binaryen -xvf binaryen-version_$BINARYEN_VERSION-$(arch)-linux.tar.gz --strip-components=1
    RUN mkdir -p /tools/bin; cp /tmp/binaryen/bin/wasm-opt /tools/bin/
    SAVE IMAGE --push $TOOLS_IMAGE/tool-binaryen:$TOOL_TAG_BINARYEN

tool-binaryen:
    IF [ "$TOOLS" = "container" ]
        FROM $TOOLS_IMAGE/tool-binaryen:$TOOL_TAG_BINARYEN
    ELSE
        FROM +tool-binaryen-build
    END
    SAVE ARTIFACT /tools/bin bin

# Flutter SDK, from the same pinned installer the dev-setup scripts use, so CI and a dev host
# land on one SDK. Built on the flutter image rather than +deps-base: nothing else needs it.
tool-flutter-build:
    FROM $FLUTTER_BUILD_IMAGE
    # Same amd64-only guard as +deps-flutter, so an arm64 dev gets this message instead of a
    # confusing failure extracting an x64 SDK.
    IF [ $(arch) != "x86_64" ]
        RUN echo "the flutter gates need an x86_64 host; Flutter ships no linux-arm64 SDK"
        RUN false
    END
    ENV DEBIAN_FRONTEND=noninteractive
    ENV FLUTTER_SDK_DIR=/usr/local/share/veilid-flutter-sdk
    RUN apt-get -y update
    RUN apt-get install -y ca-certificates curl git unzip xz-utils
    COPY --keep-ts .dagger/versions.env dev-setup/_flutter_install.sh /tmp/flutter-install/
    RUN . /tmp/flutter-install/versions.env && . /tmp/flutter-install/_flutter_install.sh && i_flutter
    SAVE IMAGE --push $TOOLS_IMAGE/tool-flutter:$TOOL_TAG_FLUTTER

tool-flutter:
    IF [ "$TOOLS" = "container" ]
        FROM $TOOLS_IMAGE/tool-flutter:$TOOL_TAG_FLUTTER
    ELSE
        FROM +tool-flutter-build
    END
    SAVE ARTIFACT /usr/local/share/veilid-flutter-sdk sdk

# Every cached tool assembled into the toolchain the linux and android builds start from
deps-rust:
    FROM +deps-base
    COPY +tool-rust/rustup $RUSTUP_HOME
    COPY +tool-rust/cargo $CARGO_HOME
    # Each tool lands in its own directory and is merged in by cp, so none of this rests on
    # what COPY does when the destination already exists.
    COPY +tool-cargo-bins/bin /tmp/tools/cargo-bins
    COPY +tool-astgrep/bin /tmp/tools/astgrep
    COPY +tool-zigbuild/bin /tmp/tools/zigbuild
    COPY +tool-binaryen/bin /tmp/tools/binaryen
    RUN cp -a /tmp/tools/*/. $CARGO_HOME/bin/ && rm -rf /tmp/tools
    COPY +tool-zig/zig /usr/local/zig
    # A missing artifact here is a silent COPY, so prove the assembled toolchain runs
    RUN rustup --version && cargo --version && rustc --version \
        && cargo-zigbuild --version && ast-grep --version && cargo nextest --version \
        && cargo binstall -V && wasm-opt --version

    SAVE ARTIFACT $RUSTUP_HOME rustup
    SAVE ARTIFACT $CARGO_HOME cargo
    SAVE ARTIFACT /usr/local/cargo/bin/cargo-zigbuild
    SAVE ARTIFACT /usr/local/zig

# Install android tooling
deps-android:
    FROM +deps-base
    WAIT
        BUILD +deps-rust
    END
    COPY +deps-rust/cargo /usr/local/cargo
    COPY +deps-rust/rustup /usr/local/rustup
    COPY +deps-rust/cargo-zigbuild /usr/local/cargo/bin/cargo-zigbuild
    COPY +deps-rust/zig /usr/local/zig
    RUN apt-get install -y openjdk-9-jdk-headless
    RUN mkdir /Android; mkdir /Android/Sdk
    RUN retry curl --retry $RETRY_COUNT --retry-connrefused -L -o /Android/cmdline-tools.zip $ANDROID_COMMAND_LINE_TOOLS
    RUN cd /Android; unzip /Android/cmdline-tools.zip
    RUN yes | /Android/cmdline-tools/bin/sdkmanager --sdk_root=/Android/Sdk build-tools\;$ANDROID_BUILD_TOOLS_VERSION ndk\;$NDK_VERSION cmake\;$ANDROID_CMAKE_VERSION platform-tools platforms\;android-$ANDROID_VERSION cmdline-tools\;latest
    RUN rm -rf /Android/cmdline-tools
    RUN apt-get clean

# Just linux build not android
# The assembled toolchain, cached as its own registry image like the tools it is built from.
# Without this every crate-cache push re-uploads the whole toolchain to deliver a 150MB crate
# layer, because deps-base + the merged tools are fresh layers the registry cannot dedupe.
deps-linux-build:
    FROM +deps-base
    WAIT
        BUILD +deps-rust
    END
    COPY +deps-rust/cargo /usr/local/cargo
    COPY +deps-rust/rustup /usr/local/rustup
    COPY +deps-rust/cargo-zigbuild /usr/local/cargo/bin/cargo-zigbuild
    COPY +deps-rust/zig /usr/local/zig
    SAVE IMAGE --push $TOOLS_IMAGE/deps-linux:$DEPS_LINUX_TAG

deps-linux:
    IF [ "$TOOLS" = "container" ]
        FROM $TOOLS_IMAGE/deps-linux:$DEPS_LINUX_TAG
    ELSE
        FROM +deps-linux-build
    END
    # Re-assert after the FROM: a published image carries the ENV it was BUILT with, and starting
    # FROM an external image replaces the base recipe's environment entirely. Without this the
    # autodetected job count is frozen at whatever the cache image was baked with -- which is
    # exactly what happened: crate-cache carried CARGO_BUILD_JOBS=1 and every test job compiled
    # single-threaded no matter how large the runner was.
    ENV CARGO_BUILD_JOBS=$CARGO_JOBS

# Flutter SDK for the veilid-flutter dart format and integration gates. Newer base than
# the rest of the build: these gates ship nothing, so the LINUX_BUILD_IMAGE glibc floor
# that keeps the release binaries portable buys them nothing, and Flutter's own floor is
# well above it.
deps-flutter-build:
    FROM $FLUTTER_BUILD_IMAGE
    # Flutter publishes a linux-x64 SDK only, so this half of the build is amd64-only
    IF [ $(arch) != "x86_64" ]
        RUN echo "the flutter integration tests need an x86_64 host; Flutter ships no linux-arm64 SDK"
        RUN false
    END
    ENV DEBIAN_FRONTEND=noninteractive
    ENV RUSTUP_HOME=/usr/local/rustup
    ENV CARGO_HOME=/usr/local/cargo
    ENV FLUTTER_SDK_DIR=/usr/local/share/veilid-flutter-sdk
    ENV PATH=$PATH:/usr/local/cargo/bin:/usr/local/share/veilid-flutter-sdk/flutter/bin
    ENV CARGO_INCREMENTAL=0
    ENV CARGO_BUILD_JOBS=$CARGO_JOBS
    ENV RUST_BACKTRACE=1
    RUN apt-get -y update
    RUN apt-get install -y ca-certificates curl git unzip xz-utils file build-essential pkg-config libssl-dev
    COPY +tool-flutter/sdk $FLUTTER_SDK_DIR
    # flutter shells out to git against its own checkout, which this container did not clone
    RUN git config --global --add safe.directory "$FLUTTER_SDK_DIR/flutter"
    RUN flutter --version
    SAVE IMAGE --push $TOOLS_IMAGE/deps-flutter:$DEPS_FLUTTER_TAG

# The flutter gates' base, cached like the linux one: it apt-installs its prerequisites onto a bare
# FLUTTER_BUILD_IMAGE and copies in a 1.8GB SDK artifact, which every flutter job otherwise redoes.
deps-flutter:
    IF [ "$TOOLS" = "container" ]
        FROM $TOOLS_IMAGE/deps-flutter:$DEPS_FLUTTER_TAG
    ELSE
        FROM +deps-flutter-build
    END
    # Re-assert after the FROM: a published image carries the ENV it was BUILT with (see deps-linux).
    ENV CARGO_BUILD_JOBS=$CARGO_JOBS

# ...and with the rust toolchain, for the integration gate that builds the native library. Copying
# it in BEFORE the source (rather than after, as code-flutter-rust used to) is what makes it
# cacheable at all.
deps-flutter-rust-build:
    FROM +deps-flutter
    COPY +tool-rust/rustup $RUSTUP_HOME
    COPY +tool-rust/cargo $CARGO_HOME
    RUN cargo --version && rustc --version
    SAVE IMAGE --push $TOOLS_IMAGE/deps-flutter-rust:$DEPS_FLUTTER_RUST_TAG

deps-flutter-rust:
    IF [ "$TOOLS" = "container" ]
        FROM $TOOLS_IMAGE/deps-flutter-rust:$DEPS_FLUTTER_RUST_TAG
    ELSE
        FROM +deps-flutter-rust-build
    END
    # Re-assert after the FROM: a published image carries the ENV it was BUILT with (see deps-linux).
    ENV CARGO_BUILD_JOBS=$CARGO_JOBS

# The cross toolchain for the prebuilt veilid-flutter Windows library: clang-cl, llvm-lib and
# lld-link, the rust msvc target, and the MSVC CRT and Windows SDK that xwin splats into /xwin.
# --accept-license is the Veilid Foundation's acceptance of Microsoft's license for the CRT and SDK
# (https://go.microsoft.com/fwlink/?LinkId=2086102).
deps-xwin-build:
    FROM +deps-base
    RUN apt-get update && apt-get install -y clang lld llvm
    # The distribution ships the MSVC-style driver names only inside the llvm tree
    RUN llvm=$(ls -d /usr/lib/llvm-*/bin | sort -V | tail -n 1) \
        && ln -s $llvm/clang /usr/local/bin/clang-cl \
        && ln -s $llvm/llvm-ar /usr/local/bin/llvm-lib \
        && ln -s $llvm/lld /usr/local/bin/lld-link
    COPY +tool-rust/rustup $RUSTUP_HOME
    COPY +tool-rust/cargo $CARGO_HOME
    RUN rustup target add x86_64-pc-windows-msvc
    RUN retry curl -fsSL -o /tmp/xwin.tar.gz https://github.com/Jake-Shadle/xwin/releases/download/$XWIN_VERSION/xwin-$XWIN_VERSION-x86_64-unknown-linux-musl.tar.gz \
        && tar -xzf /tmp/xwin.tar.gz -C /tmp --strip-components=1 xwin-$XWIN_VERSION-x86_64-unknown-linux-musl/xwin \
        && retry /tmp/xwin --accept-license --cache-dir /tmp/xwin-cache --crt-version $XWIN_CRT_VERSION --sdk-version $XWIN_SDK_VERSION splat --output /xwin \
        && rm -rf /tmp/xwin /tmp/xwin.tar.gz /tmp/xwin-cache
    ENV CC_x86_64_pc_windows_msvc=clang-cl
    ENV CXX_x86_64_pc_windows_msvc=clang-cl
    ENV AR_x86_64_pc_windows_msvc=llvm-lib
    ENV CFLAGS_x86_64_pc_windows_msvc="-Wno-unused-command-line-argument /vctoolsdir /xwin/crt /winsdkdir /xwin/sdk"
    ENV CXXFLAGS_x86_64_pc_windows_msvc="-Wno-unused-command-line-argument /vctoolsdir /xwin/crt /winsdkdir /xwin/sdk"
    ENV CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER=lld-link
    # Through LIB rather than target rustflags, which would replace .cargo/config.toml's
    ENV LIB="/xwin/crt/lib/x86_64;/xwin/sdk/lib/um/x86_64;/xwin/sdk/lib/ucrt/x86_64"
    SAVE IMAGE --push $TOOLS_IMAGE/deps-xwin:$DEPS_XWIN_TAG

deps-xwin:
    IF [ "$TOOLS" = "container" ]
        FROM $TOOLS_IMAGE/deps-xwin:$DEPS_XWIN_TAG
    ELSE
        FROM +deps-xwin-build
    END
    # Re-assert after the FROM: a published image carries the ENV it was BUILT with (see deps-linux).
    ENV CARGO_BUILD_JOBS=$CARGO_JOBS

# Code + Flutter deps
code-flutter:
    FROM +deps-flutter
    COPY --keep-ts --dir .cargo .dagger dev-setup scripts veilid-cli veilid-core veilid-server veilid-tools veilid-core-optimized veilid-flutter veilid-wasm veilid-remote-api Cargo.lock Cargo.toml clippy.toml sgconfig.yml /veilid
    WORKDIR /veilid

# The flutter gates that build veilid-flutter's native library need a rust toolchain on top.
# The dart format gate does not, and starting it from +code-flutter keeps it off this path.
code-flutter-rust:
    FROM +deps-flutter-rust
    COPY --keep-ts --dir .cargo .dagger dev-setup scripts veilid-cli veilid-core veilid-server veilid-tools veilid-core-optimized veilid-flutter veilid-wasm veilid-remote-api Cargo.lock Cargo.toml clippy.toml sgconfig.yml /veilid
    WORKDIR /veilid

# Pre-download every crate in Cargo.lock so that no later job depends on reaching crates.io.
# Manifests and the lockfile only: `cargo fetch` resolves from Cargo.lock, which records the whole
# dependency graph regardless of features, so one fetch covers every feature set the test jobs use.
# A workspace member missing from the list below fails the fetch outright rather than silently
# under-fetching.
fetch-cache:
    FROM +deps-linux
    COPY --keep-ts --dir .cargo Cargo.lock Cargo.toml .
    COPY --keep-ts veilid-cli/Cargo.toml veilid-cli/
    COPY --keep-ts veilid-core/Cargo.toml veilid-core/
    COPY --keep-ts veilid-core/examples/basic/Cargo.toml veilid-core/examples/basic/
    COPY --keep-ts veilid-core/examples/private_route/Cargo.toml veilid-core/examples/private_route/
    COPY --keep-ts veilid-server/Cargo.toml veilid-server/
    COPY --keep-ts veilid-tools/Cargo.toml veilid-tools/
    COPY --keep-ts veilid-core-optimized/Cargo.toml veilid-core-optimized/
    COPY --keep-ts veilid-remote-api/Cargo.toml veilid-remote-api/
    COPY --keep-ts veilid-flutter/rust/Cargo.toml veilid-flutter/rust/
    COPY --keep-ts veilid-wasm/Cargo.toml veilid-wasm/
    # All one RUN: deleting registry/src in a later layer would leave its bulk in this one. Order
    # matters twice over — the first delete forces the --offline re-fetch to work from the .crate
    # files alone, which is what proves the cache is complete (an image short a crate fails
    # silently downstream), and that re-fetch re-extracts src, so it has to be deleted again.
    RUN cargo fetch --locked && rm -rf $CARGO_HOME/registry/src \
        && cargo fetch --locked --offline && rm -rf $CARGO_HOME/registry/src
    # wasm-bindgen-cli's version is pinned by Cargo.lock, which is exactly what this image is keyed
    # on, so it belongs here rather than being re-installed by every test job that consumes it.
    RUN WASM_BINDGEN_VERSION=$(cargo tree --locked -p veilid-wasm -i wasm-bindgen | head -n 1 | cut -c 15-); \
        cargo binstall wasm-bindgen-cli --locked --targets $DEFAULT_CARGO_MUSL_TARGET --version $WASM_BINDGEN_VERSION || \
        cargo install wasm-bindgen-cli --locked --version $WASM_BINDGEN_VERSION
    ARG CI_REGISTRY_IMAGE=registry.gitlab.com/veilid/veilid
    ARG CRATE_CACHE_TAG
    SAVE IMAGE --push $CI_REGISTRY_IMAGE/crate-cache:$CRATE_CACHE_TAG

# Import the whole veilid code repository from the earthly host
code-linux:
    # Crates arrive pre-downloaded when CRATE_CACHE_TAG names a +fetch-cache image; otherwise the
    # build downloads them itself. Nothing compiled is cached either way.
    ARG CRATE_CACHE_TAG
    IF [ -n "$CRATE_CACHE_TAG" ]
        ARG CI_REGISTRY_IMAGE=registry.gitlab.com/veilid/veilid
        RUN echo "Using crate cache from registry: $CI_REGISTRY_IMAGE/crate-cache:$CRATE_CACHE_TAG"
        FROM $CI_REGISTRY_IMAGE/crate-cache:$CRATE_CACHE_TAG
    ELSE
        RUN echo "No crate cache, downloading crates during the build"
        FROM +deps-linux
    END
    # Re-assert after the FROM: a published image carries the ENV it was BUILT with, and starting
    # FROM an external image replaces the base recipe's environment entirely. Without this the
    # autodetected job count is frozen at whatever the cache image was baked with -- which is
    # exactly what happened: crate-cache carried CARGO_BUILD_JOBS=1 and every test job compiled
    # single-threaded no matter how large the runner was.
    ENV CARGO_BUILD_JOBS=$CARGO_JOBS
    COPY --keep-ts --dir .cargo scripts veilid-cli veilid-core veilid-server veilid-tools veilid-core-optimized veilid-flutter veilid-wasm veilid-remote-api Cargo.lock Cargo.toml clippy.toml sgconfig.yml /veilid
    # Check to make sure Cargo.lock is up to date
    RUN cargo update -w --locked
    # Restore original Cargo.lock
    COPY --keep-ts --dir Cargo.lock /veilid
    # Normally already baked into the crate cache (see +fetch-cache): a test job should not be
    # installing software. This only fires on the uncached path, where there is no image to have
    # carried it.
    IF ! command -v wasm-bindgen > /dev/null 2>&1
        RUN WASM_BINDGEN_VERSION=$(cargo tree --locked -p veilid-wasm -i wasm-bindgen | head -n 1 | cut -c 15-); \
            cargo binstall wasm-bindgen-cli --locked --targets $DEFAULT_CARGO_MUSL_TARGET --version $WASM_BINDGEN_VERSION || \
            cargo install wasm-bindgen-cli --locked --version $WASM_BINDGEN_VERSION
    END

# Code + Linux + Android deps
code-android:
    FROM +deps-android
    COPY --keep-ts --dir .cargo scripts veilid-cli veilid-core veilid-server veilid-tools veilid-core-optimized veilid-flutter veilid-wasm veilid-remote-api Cargo.lock Cargo.toml clippy.toml sgconfig.yml /veilid
    COPY --keep-ts scripts/earthly/cargo-android/config.toml /veilid/.cargo/config.toml

# Lints only
lint:
    FROM +code-linux
    RUN ./scripts/_lint_all.sh --cleanup

# MSRV only
msrv:
    FROM +code-linux
    RUN ./scripts/_msrv_all.sh        

# Smoketest only
smoketest:
    FROM +code-linux
    RUN ./scripts/_smoketest_all.sh --locked

# Public API only
public-api:
    FROM +code-linux
    RUN ./scripts/_check_public_api.sh        

# Census gate only
census:
    FROM +code-linux
    RUN ./scripts/test_census.sh
    RUN CENSUS_CHECK_ONLY=1 ./scripts/update_census.sh

# Build
build-linux-amd64:
    FROM +code-linux
    # Ensure we have enough memory
    IF [ $(free -wmt | grep Total | awk  '{print $2}') -lt 7500 ]
        RUN echo "not enough container memory to build. increase build host memory."
        RUN false
    END
    RUN cargo zigbuild --locked --target x86_64-unknown-linux-gnu.$GLIBC_FLOOR --release -p veilid-server -p veilid-cli -p veilid-tools -p veilid-core-optimized -p veilid-core -p veilid-remote-api
    SAVE ARTIFACT ./target/x86_64-unknown-linux-gnu AS LOCAL ./target/artifacts/x86_64-unknown-linux-gnu

build-linux-arm64:
    FROM +code-linux
    # Ensure we have enough memory
    IF [ $(free -wmt | grep Total | awk  '{print $2}') -lt 7500 ]
        RUN echo "not enough container memory to build. increase build host memory."
        RUN false
    END
    RUN cargo zigbuild --locked --target aarch64-unknown-linux-gnu.$GLIBC_FLOOR --release -p veilid-server -p veilid-cli -p veilid-tools -p veilid-core-optimized -p veilid-core -p veilid-remote-api
    SAVE ARTIFACT ./target/aarch64-unknown-linux-gnu AS LOCAL ./target/artifacts/aarch64-unknown-linux-gnu

# build-windows-amd64:
#     FROM +code-linux
#     # Ensure we have enough memory
#     IF [ $(free -wmt | grep Total | awk  '{print $2}') -lt 7500 ]
#         RUN echo "not enough container memory to build. increase build host memory."
#         RUN false
#     END
#     RUN cargo zigbuild --locked --target x86_64-pc-windows-gnu --release -p veilid-server -p veilid-cli -p veilid-tools -p veilid-core-optimized -p veilid-core -p veilid-remote-api
#     SAVE ARTIFACT ./target/x86_64-pc-windows-gnu AS LOCAL ./target/artifacts/x86_64-pc-windows-gnu

# build-macos-arm64:
#     FROM +code-linux
#     # Ensure we have enough memory
#     IF [ $(free -wmt | grep Total | awk  '{print $2}') -lt 7500 ]
#         RUN echo "not enough container memory to build. increase build host memory."
#         RUN false
#     END
#     RUN cargo zigbuild --locked --target aarch64-apple-darwin --release -p veilid-server -p veilid-cli -p veilid-tools -p veilid-core-optimized -p veilid-core -p veilid-remote-api
#     SAVE ARTIFACT ./target/aarch64-apple-darwin AS LOCAL ./target/artifacts/aarch64-apple-darwin


build-android:
    FROM +code-android
    WORKDIR /veilid/veilid-core
    ENV PATH=$PATH:/Android/Sdk/ndk/$NDK_VERSION/toolchains/llvm/prebuilt/linux-$(arch)/bin/
    RUN cargo build --locked --target aarch64-linux-android --release
    RUN cargo build --locked --target armv7-linux-androideabi --release
    RUN cargo build --locked --target i686-linux-android --release
    RUN cargo build --locked --target x86_64-linux-android --release
    WORKDIR /veilid
    SAVE ARTIFACT ./target/aarch64-linux-android AS LOCAL ./target/artifacts/aarch64-linux-android
    SAVE ARTIFACT ./target/armv7-linux-androideabi AS LOCAL ./target/artifacts/armv7-linux-androideabi
    SAVE ARTIFACT ./target/i686-linux-android AS LOCAL ./target/artifacts/i686-linux-android
    SAVE ARTIFACT ./target/x86_64-linux-android AS LOCAL ./target/artifacts/x86_64-linux-android

# Unit tests
test-msrv-all:
    FROM +code-linux
    RUN ./scripts/_msrv_all.sh
test-smoketest-all:
    FROM +code-linux
    RUN ./scripts/_smoketest_all.sh --locked
test-public-api:
    FROM +code-linux
    RUN ./scripts/_check_public_api.sh

test-census:
    FROM +code-linux
    RUN ./scripts/test_census.sh
    RUN CENSUS_CHECK_ONLY=1 ./scripts/update_census.sh
test-check-wasm-builds-dart:
    FROM +code-linux
    RUN ./scripts/_check_wasm_builds.sh dart
test-check-wasm-builds-js:
    FROM +code-linux
    RUN ./scripts/_check_wasm_builds.sh js
# eslint's type-aware rules read the generated pkg/veilid_wasm.d.ts, so the wasm package comes first
test-lint-typescript:
    FROM +code-linux
    RUN ./scripts/_check_wasm_builds.sh js
    RUN ./scripts/_lint_typescript.sh
test-clippy-wasm32-unknown-unknown:
    FROM +code-linux
    RUN ./scripts/_clippy_wasm32_unknown_unknown.sh
test-clippy-x86-64-pc-windows-gnu:
    FROM +code-linux
    RUN ./scripts/_clippy_x86_64_pc_windows_gnu.sh
test-clippy-aarch64-apple-darwin:
    FROM +code-linux
    RUN ./scripts/_clippy_aarch64_apple_darwin.sh
test-clippy-x86-64-unknown-linux-gnu:
    FROM +code-linux
    RUN ./scripts/_clippy_x86_64_unknown_linux_gnu.sh
test-clippy-x86-64-unknown-linux-gnu-async-std:
    FROM +code-linux
    RUN ./scripts/_clippy_x86_64_unknown_linux_gnu_async_std.sh
test-clippy-mock-api:
    FROM +code-linux
    RUN ./scripts/_clippy_mock_api.sh
test-clippy-optional-features:
    FROM +code-linux
    RUN ./scripts/_clippy_optional_features.sh
test-build-docs:
    FROM +code-linux    
    RUN ./scripts/_build_docs.sh
test-unit-tests-all:
    FROM +code-linux
    RUN ./scripts/_unit_tests_all.sh
# The desktop flutter binding end to end against the in-process mock, headless on
# flutter-tester - the same run check_all.sh does locally.
test-dart-format:
    FROM +code-flutter
    RUN ./scripts/_check_format_dart.sh
test-readme-sync:
    FROM +code-linux
    RUN ./scripts/check_readme_sync.sh
test-flutter-integration:
    FROM +code-flutter-rust
    RUN ./scripts/_flutter_integration_tests.sh
test-all:
    BUILD +test-census
    BUILD +test-dart-format
    BUILD +test-readme-sync
    BUILD +test-msrv-all
    BUILD +test-smoketest-all
    BUILD +test-public-api
    BUILD +test-check-wasm-builds-dart
    BUILD +test-check-wasm-builds-js
    BUILD +test-lint-typescript
    BUILD +test-clippy-wasm32-unknown-unknown
    BUILD +test-clippy-x86-64-pc-windows-gnu
    BUILD +test-clippy-aarch64-apple-darwin
    BUILD +test-clippy-x86-64-unknown-linux-gnu
    BUILD +test-clippy-x86-64-unknown-linux-gnu-async-std
    BUILD +test-clippy-mock-api
    BUILD +test-clippy-optional-features
    BUILD +test-build-docs
    BUILD +test-unit-tests-all
    BUILD +test-flutter-integration

# The .deb's SELinux policy module, compiled on its own image rather than in the build container.
# checkmodule emits the newest policy version its refpolicy knows and a module will not load on a
# system with an older libsepol, so this floor is a support decision (SELINUX_POLICY_IMAGE) and
# should not drift with whatever the compiler happens to run on. The RPM path builds its own.
selinux-policy-deb:
    FROM $SELINUX_POLICY_IMAGE
    RUN apt-get -y update
    RUN DEBIAN_FRONTEND=noninteractive apt-get install -y selinux-policy-dev bzip2 make
    COPY --keep-ts --dir package/selinux /work/
    RUN cd /work/selinux && ./build_module.sh
    SAVE ARTIFACT /work/selinux/veilid_server.pp.bz2

# Package
# Each package compiles the SELinux .pp in-place from package/selinux (RPM on rockylinux:9, DEB on ubuntu 18.04).
package-linux-amd64-deb:
    ARG IS_NIGHTLY="false"
    FROM +build-linux-amd64
    #################################
    ### DEBIAN DPKG .DEB FILES
    #################################
    COPY --keep-ts --dir package /veilid
    COPY +selinux-policy-deb/veilid_server.pp.bz2 /veilid/package/selinux/
    # veilid-server
    RUN /veilid/package/debian/earthly_make_veilid_server_deb.sh amd64 x86_64-unknown-linux-gnu "$IS_NIGHTLY" "$GLIBC_FLOOR"
    SAVE ARTIFACT --keep-ts /dpkg/out/*.deb AS LOCAL ./target/packages/
    # veilid-cli
    RUN /veilid/package/debian/earthly_make_veilid_cli_deb.sh amd64 x86_64-unknown-linux-gnu "$IS_NIGHTLY" "$GLIBC_FLOOR"
    # save artifacts
    SAVE ARTIFACT --keep-ts /dpkg/out/*.deb AS LOCAL ./target/packages/

package-linux-amd64-rpm:
    ARG IS_NIGHTLY="false"
    FROM --platform linux/amd64 rockylinux:9
    RUN yum install -y createrepo rpm-build rpm-sign yum-utils rpmdevtools selinux-policy-devel bzip2 make
    RUN rpmdev-setuptree
    #################################
    ### RPMBUILD .RPM FILES
    #################################
    RUN mkdir -p /veilid/target
    RUN mkdir -p /veilid/veilid-cli /veilid/veilid-server
    COPY --keep-ts veilid-cli/Cargo.toml /veilid/veilid-cli
    COPY --keep-ts veilid-server/Cargo.toml /veilid/veilid-server
    COPY --keep-ts --dir package /veilid
    COPY --keep-ts +build-linux-amd64/x86_64-unknown-linux-gnu /veilid/target/x86_64-unknown-linux-gnu
    # Compile the SELinux policy module here (RHEL-family .pp; forward-compatible to Fedora)
    RUN cd /veilid/package/selinux && ./build_module.sh
    RUN mkdir -p /rpm-work-dir/veilid-server
    # veilid-server
    RUN veilid/package/rpm/veilid-server/earthly_make_veilid_server_rpm.sh x86_64 x86_64-unknown-linux-gnu "$IS_NIGHTLY"
    #SAVE ARTIFACT --keep-ts /root/rpmbuild/RPMS/x86_64/*.rpm AS LOCAL ./target/packages/
    # veilid-cli
    RUN veilid/package/rpm/veilid-cli/earthly_make_veilid_cli_rpm.sh x86_64 x86_64-unknown-linux-gnu "$IS_NIGHTLY"
    # save artifacts
    SAVE ARTIFACT --keep-ts /root/rpmbuild/RPMS/x86_64/*.rpm AS LOCAL ./target/packages/

package-linux-arm64-deb:
    ARG IS_NIGHTLY="false"
    FROM +build-linux-arm64
    #################################
    ### DEBIAN DPKG .DEB FILES
    #################################
    COPY --keep-ts --dir package /veilid
    COPY +selinux-policy-deb/veilid_server.pp.bz2 /veilid/package/selinux/
    # veilid-server
    RUN /veilid/package/debian/earthly_make_veilid_server_deb.sh arm64 aarch64-unknown-linux-gnu "$IS_NIGHTLY" "$GLIBC_FLOOR"
    SAVE ARTIFACT --keep-ts /dpkg/out/*.deb AS LOCAL ./target/packages/
    # veilid-cli
    RUN /veilid/package/debian/earthly_make_veilid_cli_deb.sh arm64 aarch64-unknown-linux-gnu "$IS_NIGHTLY" "$GLIBC_FLOOR"
    # save artifacts
    SAVE ARTIFACT --keep-ts /dpkg/out/*.deb AS LOCAL ./target/packages/

package-linux-arm64-rpm:
    ARG IS_NIGHTLY="false"
    FROM --platform linux/arm64 rockylinux:9
    RUN yum install -y createrepo rpm-build rpm-sign yum-utils rpmdevtools selinux-policy-devel bzip2 make
    RUN rpmdev-setuptree
    #################################
    ### RPMBUILD .RPM FILES
    #################################
    RUN mkdir -p /veilid/target
    RUN mkdir -p /veilid/veilid-cli /veilid/veilid-server
    COPY --keep-ts veilid-cli/Cargo.toml /veilid/veilid-cli
    COPY --keep-ts veilid-server/Cargo.toml /veilid/veilid-server
    COPY --keep-ts --dir package /veilid
    COPY --keep-ts +build-linux-arm64/aarch64-unknown-linux-gnu /veilid/target/aarch64-unknown-linux-gnu
    # Compile the SELinux policy module here (RHEL-family .pp; forward-compatible to Fedora)
    RUN cd /veilid/package/selinux && ./build_module.sh
    RUN mkdir -p /rpm-work-dir/veilid-server
    # veilid-server
    RUN veilid/package/rpm/veilid-server/earthly_make_veilid_server_rpm.sh aarch64 aarch64-unknown-linux-gnu "$IS_NIGHTLY"
    #SAVE ARTIFACT --keep-ts /root/rpmbuild/RPMS/aarch64/*.rpm AS LOCAL ./target/packages/
    # veilid-cli
    RUN veilid/package/rpm/veilid-cli/earthly_make_veilid_cli_rpm.sh aarch64 aarch64-unknown-linux-gnu "$IS_NIGHTLY"
    # save artifacts
    SAVE ARTIFACT --keep-ts /root/rpmbuild/RPMS/aarch64/*.rpm AS LOCAL ./target/packages/

package-linux-amd64:
    WAIT
        BUILD +package-linux-amd64-deb
    END
    WAIT
        BUILD +package-linux-amd64-rpm
    END

package-linux-arm64:
    WAIT
        BUILD +package-linux-arm64-deb
    END
    WAIT
        BUILD +package-linux-arm64-rpm
    END

package-tests-docs-linux:
    FROM +code-linux
    RUN ./scripts/_build_docs.sh $RUST_PACKAGE_TESTS_NIGHTLY_VERSION

package-linux:
    WAIT
        BUILD +package-tests-docs-linux
    END
    WAIT
        BUILD +package-linux-amd64
    END
    WAIT
        BUILD +package-linux-arm64
    END

# The prebuilt veilid-flutter library a Windows x64 app build bundles in place of compiling it
# (veilid-flutter/hook/prebuilt.dart): release, default features, with its SHA-256 and a manifest
# for the prebuilt_flutter_windows_x64 job to publish.
prebuilt-flutter-windows-x64:
    FROM +deps-xwin
    COPY --keep-ts --dir .cargo scripts veilid-cli veilid-core veilid-server veilid-tools veilid-core-optimized veilid-flutter veilid-wasm veilid-remote-api Cargo.lock Cargo.toml /veilid
    WORKDIR /veilid
    ARG --required COMMIT
    ARG BUILD_HOST=local
    RUN cargo build --release --locked -p veilid-flutter --target x86_64-pc-windows-msvc
    RUN mkdir -p /out \
        && cp target/x86_64-pc-windows-msvc/release/veilid_flutter.dll /out/ \
        && sha256sum /out/veilid_flutter.dll | cut -d' ' -f1 > /out/veilid_flutter.dll.sha256 \
        && jq -n \
            --arg commit "$COMMIT" \
            --arg version "$(sed -n 's/^version = "\(.*\)"$/\1/p' veilid-flutter/rust/Cargo.toml | head -n 1)-$(printf %.12s "$COMMIT")" \
            --arg rustc "$(rustc --version)" \
            --arg xwin "$XWIN_VERSION" \
            --arg msvc_crt "$XWIN_CRT_VERSION" \
            --arg windows_sdk "$XWIN_SDK_VERSION" \
            --arg build_host "$BUILD_HOST" \
            '{commit: $commit, version: $version, target: "x86_64-pc-windows-msvc", features: "default", rustc: $rustc, xwin: $xwin, msvc_crt: $msvc_crt, windows_sdk: $windows_sdk, build_host: $build_host}' \
            > /out/manifest.json
    SAVE ARTIFACT /out/* AS LOCAL target/prebuilt/flutter-windows-x64/
