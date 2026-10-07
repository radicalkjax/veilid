VERSION 0.8

########################################################################################################################
## ARGUMENTS
##
## CI_REGISTRY_IMAGE - used so that forks can refer to themselves, e.g. to use the fork's own registry cache in the
## `+build-linux-cache` target, and defaulting to `registry.gitlab.com/veilid/veilid` if not specified
##
## BASE - tells the build whether it should run in the default mode which runs the complete build, or run by starting
## with the remote `container` value which uses `$CACHE_NAME:$CACHE_TAG` as set up in the projects Container Registry
##
########################################################################################################################

# Start with older Ubuntu to ensure GLIBC symbol versioning support for older linux
# Ensure we are using an amd64 platform because some of these targets use cross-platform tooling
FROM ubuntu:18.04
# Tool versions come from the single source of truth, .dagger/versions.env (the
# dev-setup scripts source the same file). Read here as global ARGs via command
# substitution so every RUN/ENV below uses them unchanged.
COPY .dagger/versions.env /etc/veilid-versions.env
ARG --global ZIG_VERSION=$(. /etc/veilid-versions.env && echo $ZIG_VERSION)
ARG --global CMAKE_VERSION_MINOR=$(. /etc/veilid-versions.env && echo $CMAKE_VERSION_MINOR)
ARG --global CMAKE_VERSION_PATCH=$(. /etc/veilid-versions.env && echo $CMAKE_VERSION_PATCH)
ARG --global BINARYEN_VERSION=$(. /etc/veilid-versions.env && echo $BINARYEN_VERSION)
ARG --global RUST_VERSION=$(. /etc/veilid-versions.env && echo $RUST_VERSION)
ARG --global RUST_UNIT_TESTS_NIGHTLY_VERSION=$(. /etc/veilid-versions.env && echo $RUST_UNIT_TESTS_NIGHTLY_VERSION)
ARG --global RUST_PACKAGE_TESTS_NIGHTLY_VERSION=$(. /etc/veilid-versions.env && echo $RUST_PACKAGE_TESTS_NIGHTLY_VERSION)
ARG --global NDK_VERSION=$(. /etc/veilid-versions.env && echo $ANDROID_NDK_VERSION)
ARG --global ANDROID_VERSION=$(. /etc/veilid-versions.env && echo $ANDROID_PLATFORM_VERSION)
ARG --global ANDROID_BUILD_TOOLS_VERSION=$(. /etc/veilid-versions.env && echo $ANDROID_BUILD_TOOLS_VERSION)
ARG --global ANDROID_CMDLINE_TOOLS_VERSION=$(. /etc/veilid-versions.env && echo $ANDROID_CMDLINE_TOOLS_VERSION)
ARG --global RUST_STABLE_TARGETS=$(. /etc/veilid-versions.env && echo $RUST_STABLE_TARGETS)
ARG --global RUST_NIGHTLY_TARGETS=$(. /etc/veilid-versions.env && echo $RUST_NIGHTLY_TARGETS)
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
# Limit parallel jobs to match medium runner resources (2 vCPU, 8GB RAM)
ENV CARGO_BUILD_JOBS=1
    

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

# Install build prerequisites & setup required directories
deps-base:
    RUN echo '\
        Acquire::Retries "'$RETRY_COUNT'";\
        Acquire::https::Timeout "240";\
        Acquire::http::Timeout "240";\
        APT::Get::Assume-Yes "true";\
        APT::Install-Recommends "false";\
        APT::Install-Suggests "false";\
        Debug::Acquire::https "true";\
        ' > /etc/apt/apt.conf.d/99custom
    RUN apt-get -y update
    RUN apt-get install -y ca-certificates iproute2 curl build-essential libssl-dev openssl file git pkg-config libdbus-1-dev libdbus-glib-1-dev libgirepository1.0-dev libcairo2-dev checkinstall unzip libncursesw5-dev libncurses5-dev jq selinux-policy-dev bzip2
    IF [ $(arch) = "x86_64" ]
        RUN apt-get install -y gcc-aarch64-linux-gnu
    ELSE IF [ $(arch) = "aarch64" ]
        RUN apt-get install -y gcc-x86-64-linux-gnu
    ELSE
        RUN apt-get install -y gcc-aarch64-linux-gnu gcc-x86-64-linux-gnu
    END
    RUN curl --retry $RETRY_COUNT --retry-connrefused -O https://cmake.org/files/v$CMAKE_VERSION_MINOR/cmake-$CMAKE_VERSION_PATCH-linux-$(arch).sh
    RUN mkdir /opt/cmake
    RUN sh cmake-$CMAKE_VERSION_PATCH-linux-$(arch).sh --skip-license --prefix=/opt/cmake
    RUN ln -s /opt/cmake/bin/cmake /usr/local/bin/cmake

# Install Rust
deps-rust:
    FROM +deps-base
    RUN curl --retry $RETRY_COUNT --retry-connrefused --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --default-toolchain=$RUST_VERSION -y -c clippy --no-modify-path --profile minimal
    RUN chmod -R a+w $RUSTUP_HOME $CARGO_HOME; \
        rustup --version; \
        cargo --version; \
        rustc --version;
    # Targets come from versions.env (RUST_STABLE_TARGETS); unquoted so the shell word-splits.
    RUN retry=0; until [ "$retry" -ge $RETRY_COUNT ]; do \
            rustup target add $RUST_STABLE_TARGETS \
            && break; \
            retry=$((retry+1)); \
            echo "retry #$retry..."; \
            sleep 10; \
        done
    RUN retry=0; until [ "$retry" -ge $RETRY_COUNT ]; do \
            rustup target add --toolchain nightly $RUST_NIGHTLY_TARGETS \
            && break; \
            retry=$((retry+1)); \
            echo "retry #$retry..."; \
            sleep 10; \
        done
    RUN retry=0; until [ "$retry" -ge $RETRY_COUNT ]; do \
            rustup toolchain install $RUST_UNIT_TESTS_NIGHTLY_VERSION -c miri,rust-src && break; \
            retry=$((retry+1)); \
            echo "retry #$retry..."; \
            sleep 10; \
        done
    # Install cargo-binstall
    RUN curl --retry $RETRY_COUNT --retry-connrefused -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash
    
    # Install cargo tools (try binstall of musl targets first, then fallback to regular install)
    # (Doing it this way so we don't have to install the musl rust targets, which binstall will default to building with)

    # Caching tool (custom build supports zigbuild clippy)
    RUN curl --retry $RETRY_COUNT --retry-connrefused -L --proto '=https' --tlsv1.2 -sSf https://gitlab.com/api/v4/projects/79785586/packages/generic/cargo-chef/0.1.73/cargo-chef-x86_64-unknown-linux-musl.tar.gz | tar -xz -C $CARGO_HOME/bin
    # RUN cargo install cargo-chef --git https://gitlab.com/veilid/cargo-chef.git --locked
    # MSRV checking tool
    RUN cargo binstall cargo-msrv --disable-strategies=compile --targets $DEFAULT_CARGO_MUSL_TARGET || \
        cargo install cargo-msrv --locked
    # Nextest tool
    RUN cargo binstall cargo-nextest --disable-strategies=compile --targets $DEFAULT_CARGO_MUSL_TARGET || \
        cargo install cargo-nextest --locked
    # Docs-rs tool
    RUN cargo binstall cargo-docs-rs --disable-strategies=compile --targets $DEFAULT_CARGO_MUSL_TARGET || \
        cargo install cargo-docs-rs --locked
    # Public API checking tool
    RUN cargo binstall cargo-public-api --disable-strategies=compile --targets $DEFAULT_CARGO_MUSL_TARGET || \
        cargo install cargo-public-api --locked
    # Install Linux cross-platform tooling (no binstall available for zigbuild yet)
    RUN curl --retry $RETRY_COUNT --retry-connrefused -L -O https://ziglang.org/download/$ZIG_VERSION/zig-linux-$(arch)-$ZIG_VERSION.tar.xz
    RUN tar -C /usr/local -xJf zig-linux-$(arch)-$ZIG_VERSION.tar.xz
    RUN mv /usr/local/zig-linux-$(arch)-$ZIG_VERSION /usr/local/zig
    RUN cargo install cargo-zigbuild --locked
    # Install wasm-opt
    RUN curl --retry $RETRY_COUNT --retry-connrefused -L -O https://github.com/WebAssembly/binaryen/releases/download/version_$BINARYEN_VERSION/binaryen-version_$BINARYEN_VERSION-$(arch)-linux.tar.gz
    RUN mkdir /tmp/binaryen
    RUN tar -C /tmp/binaryen -xvf binaryen-version_$BINARYEN_VERSION-$(arch)-linux.tar.gz --strip-components=1
    RUN cp /tmp/binaryen/bin/wasm-opt $CARGO_HOME/bin

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
    RUN curl --retry $RETRY_COUNT --retry-connrefused -L -o /Android/cmdline-tools.zip $ANDROID_COMMAND_LINE_TOOLS
    RUN cd /Android; unzip /Android/cmdline-tools.zip
    RUN yes | /Android/cmdline-tools/bin/sdkmanager --sdk_root=/Android/Sdk build-tools\;$ANDROID_BUILD_TOOLS_VERSION ndk\;$NDK_VERSION cmake\;$CMAKE_VERSION_PATCH platform-tools platforms\;android-$ANDROID_VERSION cmdline-tools\;latest
    RUN rm -rf /Android/cmdline-tools
    RUN apt-get clean

# Just linux build not android
deps-linux:
    FROM +deps-base
    WAIT
        BUILD +deps-rust
    END
    COPY +deps-rust/cargo /usr/local/cargo
    COPY +deps-rust/rustup /usr/local/rustup
    COPY +deps-rust/cargo-zigbuild /usr/local/cargo/bin/cargo-zigbuild
    COPY +deps-rust/zig /usr/local/zig

# Make a cache image with downloaded and built dependencies
build-linux-cache:
    FROM +deps-linux
    RUN mkdir -p veilid-cli veilid-core veilid-core/examples/basic veilid-core/examples/private_route veilid-server veilid-tools veilid-wasm veilid-flutter veilid-flutter/rust veilid-remote-api
    COPY --keep-ts --dir .cargo scripts Cargo.lock Cargo.toml .
    COPY --keep-ts veilid-cli/Cargo.toml veilid-cli
    COPY --keep-ts veilid-core/Cargo.toml veilid-core
    COPY --keep-ts veilid-core/examples/basic/Cargo.toml veilid-core/examples/basic
    COPY --keep-ts veilid-core/examples/private_route/Cargo.toml veilid-core/examples/private_route
    COPY --keep-ts veilid-server/Cargo.toml veilid-server
    COPY --keep-ts veilid-tools/Cargo.toml veilid-tools
    COPY --keep-ts veilid-remote-api/Cargo.toml veilid-remote-api
    COPY --keep-ts veilid-flutter/rust/Cargo.toml veilid-flutter/rust
    COPY --keep-ts veilid-wasm/Cargo.toml veilid-wasm
    RUN cargo chef prepare --recipe-path recipe.json
    RUN cargo chef cook --recipe-path recipe.json --profile=test --tests --target $DEFAULT_CARGO_TARGET --all-targets --locked  -p veilid-server -p veilid-cli -p veilid-tools -p veilid-core -p veilid-remote-api
    RUN cargo chef cook --recipe-path recipe.json --zigbuild=clippy --target x86_64-unknown-linux-gnu --all-targets --locked -p veilid-server -p veilid-cli -p veilid-tools -p veilid-core -p veilid-remote-api
    RUN cargo chef cook --recipe-path recipe.json --zigbuild=clippy --target aarch64-unknown-linux-gnu --all-targets --locked -p veilid-server -p veilid-cli -p veilid-tools -p veilid-core -p veilid-remote-api
    RUN cargo chef cook --recipe-path recipe.json --zigbuild=clippy --target x86_64-pc-windows-gnu --all-targets --locked -p veilid-server -p veilid-cli -p veilid-tools -p veilid-core -p veilid-remote-api
    RUN cargo chef cook --recipe-path recipe.json --zigbuild=clippy --target aarch64-apple-darwin --all-targets --locked -p veilid-server -p veilid-cli -p veilid-tools -p veilid-core -p veilid-remote-api
    RUN cargo chef cook --recipe-path recipe.json --zigbuild=clippy --target wasm32-unknown-unknown --all-targets --locked -p veilid-wasm
    RUN cargo chef cook --recipe-path recipe.json --zigbuild=clippy --target wasm32-unknown-unknown --all-targets --locked --no-default-features --features=default-wasm -p veilid-flutter
    ARG CI_REGISTRY_IMAGE=registry.gitlab.com/veilid/veilid
    ARG CACHE_NAME
    ARG CACHE_TAG
    SAVE IMAGE --push $CI_REGISTRY_IMAGE/$CACHE_NAME:$CACHE_TAG

# Import the whole veilid code repository from the earthly host
code-linux:
    # This target will either use the full earthly cache of local use (+build-linux-cache), or will use a containerized
    # version of the +build-linux-cache from the registry
    ARG BASE=local
    IF [ "$BASE" = "local" ]
        RUN echo "Using local cache"
        FROM +build-linux-cache
    ELSE IF [ "$BASE" = "uncached" ]
        RUN echo "Not using cache"
        FROM +deps-linux
    ELSE
        ARG CI_REGISTRY_IMAGE=registry.gitlab.com/veilid/veilid
        ARG CACHE_NAME
        ARG CACHE_TAG
        RUN echo "Using cache from registry: $CI_REGISTRY_IMAGE/$CACHE_NAME:$CACHE_TAG"
        FROM $CI_REGISTRY_IMAGE/$CACHE_NAME:$CACHE_TAG
    END
    COPY --keep-ts --dir .cargo scripts veilid-cli veilid-core veilid-server veilid-tools veilid-flutter veilid-wasm veilid-remote-api Cargo.lock Cargo.toml /veilid
    # Check to make sure Cargo.lock is up to date
    RUN cargo update -w --locked
    # Restore original Cargo.lock
    COPY --keep-ts --dir Cargo.lock /veilid
    # Install the wasm-bindgen CLI tool that we need, which depends on the Cargo.lock version of wasm-bindgen being used
    RUN WASM_BINDGEN_VERSION=$(cargo tree --locked -p veilid-wasm -i wasm-bindgen | head -n 1 | cut -c 15-); \
        cargo binstall wasm-bindgen-cli --locked --targets $DEFAULT_CARGO_MUSL_TARGET --version $WASM_BINDGEN_VERSION || \
        cargo install wasm-bindgen-cli --locked --version $WASM_BINDGEN_VERSION

# Code + Linux + Android deps
code-android:
    FROM +deps-android
    COPY --keep-ts --dir .cargo scripts veilid-cli veilid-core veilid-server veilid-tools veilid-flutter veilid-wasm veilid-remote-api Cargo.lock Cargo.toml /veilid
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

# Build
build-linux-amd64:
    FROM +code-linux
    # Ensure we have enough memory
    IF [ $(free -wmt | grep Total | awk  '{print $2}') -lt 7500 ]
        RUN echo "not enough container memory to build. increase build host memory."
        RUN false
    END
    RUN cargo zigbuild --locked --target x86_64-unknown-linux-gnu --release -p veilid-server -p veilid-cli -p veilid-tools -p veilid-core -p veilid-remote-api
    SAVE ARTIFACT ./target/x86_64-unknown-linux-gnu AS LOCAL ./target/artifacts/x86_64-unknown-linux-gnu

build-linux-arm64:
    FROM +code-linux
    # Ensure we have enough memory
    IF [ $(free -wmt | grep Total | awk  '{print $2}') -lt 7500 ]
        RUN echo "not enough container memory to build. increase build host memory."
        RUN false
    END
    RUN cargo zigbuild --locked --target aarch64-unknown-linux-gnu --release -p veilid-server -p veilid-cli -p veilid-tools -p veilid-core -p veilid-remote-api
    SAVE ARTIFACT ./target/aarch64-unknown-linux-gnu AS LOCAL ./target/artifacts/aarch64-unknown-linux-gnu

# build-windows-amd64:
#     FROM +code-linux
#     # Ensure we have enough memory
#     IF [ $(free -wmt | grep Total | awk  '{print $2}') -lt 7500 ]
#         RUN echo "not enough container memory to build. increase build host memory."
#         RUN false
#     END
#     RUN cargo zigbuild --locked --target x86_64-pc-windows-gnu --release -p veilid-server -p veilid-cli -p veilid-tools -p veilid-core -p veilid-remote-api
#     SAVE ARTIFACT ./target/x86_64-pc-windows-gnu AS LOCAL ./target/artifacts/x86_64-pc-windows-gnu

# build-macos-arm64:
#     FROM +code-linux
#     # Ensure we have enough memory
#     IF [ $(free -wmt | grep Total | awk  '{print $2}') -lt 7500 ]
#         RUN echo "not enough container memory to build. increase build host memory."
#         RUN false
#     END
#     RUN cargo zigbuild --locked --target aarch64-apple-darwin --release -p veilid-server -p veilid-cli -p veilid-tools -p veilid-core -p veilid-remote-api
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
test-check-wasm-builds-dart:
    FROM +code-linux
    RUN ./scripts/_check_wasm_builds.sh dart
test-check-wasm-builds-js:
    FROM +code-linux
    RUN ./scripts/_check_wasm_builds.sh js
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
test-build-docs:
    FROM +code-linux    
    RUN ./scripts/_build_docs.sh
test-unit-tests-all:
    FROM +code-linux
    RUN ./scripts/_unit_tests_all.sh
test-all:
    BUILD +test-msrv-all
    BUILD +test-smoketest-all
    BUILD +test-public-api
    BUILD +test-check-wasm-builds-dart
    BUILD +test-check-wasm-builds-js
    BUILD +test-clippy-wasm32-unknown-unknown
    BUILD +test-clippy-x86-64-pc-windows-gnu
    BUILD +test-clippy-aarch64-apple-darwin
    BUILD +test-clippy-x86-64-unknown-linux-gnu
    BUILD +test-clippy-x86-64-unknown-linux-gnu-async-std
    BUILD +test-build-docs
    BUILD +test-unit-tests-all

# Package
# Each package compiles the SELinux .pp in-place from package/selinux (RPM on rockylinux:9, DEB on ubuntu 18.04).
package-linux-amd64-deb:
    ARG IS_NIGHTLY="false"
    FROM +build-linux-amd64
    #################################
    ### DEBIAN DPKG .DEB FILES
    #################################
    COPY --keep-ts --dir package /veilid
    RUN cd /veilid/package/selinux && ./build_module.sh
    # veilid-server
    RUN /veilid/package/debian/earthly_make_veilid_server_deb.sh amd64 x86_64-unknown-linux-gnu "$IS_NIGHTLY"
    SAVE ARTIFACT --keep-ts /dpkg/out/*.deb AS LOCAL ./target/packages/
    # veilid-cli
    RUN /veilid/package/debian/earthly_make_veilid_cli_deb.sh amd64 x86_64-unknown-linux-gnu "$IS_NIGHTLY"
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
    RUN cd /veilid/package/selinux && ./build_module.sh
    # veilid-server
    RUN /veilid/package/debian/earthly_make_veilid_server_deb.sh arm64 aarch64-unknown-linux-gnu "$IS_NIGHTLY"
    SAVE ARTIFACT --keep-ts /dpkg/out/*.deb AS LOCAL ./target/packages/
    # veilid-cli
    RUN /veilid/package/debian/earthly_make_veilid_cli_deb.sh arm64 aarch64-unknown-linux-gnu "$IS_NIGHTLY"
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