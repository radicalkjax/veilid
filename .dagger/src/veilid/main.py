import sys
from typing import Annotated
from pathlib import Path
import os
import importlib.machinery
import importlib.util

import dagger
from dagger import Ignore, dag, function, object_type
import platform

from .ignore_patterns import DIRECTORY_IGNORE_PATTERNS


def _versions_env_path():
    return os.path.join(os.path.dirname(__file__), "..", "..", "versions.env")


def _load_versions():
    """Single source of truth: .dagger/versions.env (polyglot KEY="value").

    The module runtime mounts only .dagger, so versions.env lives there and is
    imported by full path — the same file dev-setup and the Earthfile read.
    """
    path = _versions_env_path()
    loader = importlib.machinery.SourceFileLoader("_versions", path)  # .env isn't .py
    spec = importlib.util.spec_from_loader("_versions", loader)
    mod = importlib.util.module_from_spec(spec)
    loader.exec_module(mod)
    return mod


_V = _load_versions()
_VERSIONS_ENV_TEXT = Path(_versions_env_path()).read_text()

VEILID_REPO = "registry.gitlab.com/veilid/veilid"
ZIG_VERSION = _V.ZIG_VERSION
BINARYEN_VERSION = _V.BINARYEN_VERSION
RUST_VERSION = _V.RUST_VERSION
RUST_UNIT_TESTS_NIGHTLY_VERSION = _V.RUST_UNIT_TESTS_NIGHTLY_VERSION
RUST_PACKAGE_TESTS_NIGHTLY_VERSION = _V.RUST_PACKAGE_TESTS_NIGHTLY_VERSION
AST_GREP_VERSION = _V.AST_GREP_VERSION
GLIBC_FLOOR = _V.GLIBC_FLOOR
CARGO_ZIGBUILD_VERSION = _V.CARGO_ZIGBUILD_VERSION
CARGO_MSRV_VERSION = _V.CARGO_MSRV_VERSION
CARGO_NEXTEST_VERSION = _V.CARGO_NEXTEST_VERSION
CARGO_DOCS_RS_VERSION = _V.CARGO_DOCS_RS_VERSION
CARGO_PUBLIC_API_VERSION = _V.CARGO_PUBLIC_API_VERSION
CARGO_SYNC_README_VERSION = _V.CARGO_SYNC_README_VERSION
FLUTTER_BUILD_IMAGE = _V.FLUTTER_BUILD_IMAGE
RUST_STABLE_TARGETS = _V.RUST_STABLE_TARGETS.split()
RUST_NIGHTLY_TARGETS = _V.RUST_NIGHTLY_TARGETS.split()
LINUX_BUILD_IMAGE = _V.LINUX_BUILD_IMAGE
SELINUX_POLICY_IMAGE = _V.SELINUX_POLICY_IMAGE
RPM_BUILD_IMAGE = _V.RPM_BUILD_IMAGE
PACKAGING_IMAGE = _V.PACKAGING_IMAGE
RETRY_COUNT = "12"
# Retry wrapper for flaky network steps. curl's own --retry does not cover these: it
# ignores TLS handshake failures, and for `curl | sh` it only guards fetching the script.
RETRY_SCRIPT = """#!/bin/sh
i=1
while ! "$@"; do
    if [ "$i" -ge "${RETRY_COUNT:-12}" ]; then echo "giving up after $i attempts: $*" >&2; exit 1; fi
    echo "retry #$i: $*" >&2
    i=$((i+1))
    sleep 10
done
"""
RUSTUP_HOME = "/usr/local/rustup"
RUSTUP_DIST_SERVER = "https://static.rust-lang.org"
CARGO_HOME = "/usr/local/cargo"
# Disable incremental compilation in CI - useless in ephemeral containers and wastes disk
CARGO_INCREMENTAL = "0"
# Reduce debug info to line-tables-only (still gives backtraces, much smaller binaries)
CARGO_PROFILE_TEST_DEBUGINFO = "1"


def _build_jobs() -> str:
    """How many crates to compile in parallel, scaled to this machine.

    Mirrors scripts/build_jobs.sh, which the Earthly path uses: every rust job is compile-bound
    (test_unit_tests_all measured 732s compiling against 37s running tests), so this is what lets
    a larger machine actually pay for itself. The dagger engine runs on this host, so the host's
    own cpu/memory is the right thing to read. Budget 2GB per concurrent rustc -- two on
    veilid-core is a real OOM risk.
    """
    cpus = os.cpu_count() or 1
    try:
        mem_gb = os.sysconf("SC_PHYS_PAGES") * os.sysconf("SC_PAGE_SIZE") // (1024**3)
        cpus = min(cpus, max(1, mem_gb // 2))
    except (ValueError, OSError, AttributeError):
        pass
    return str(max(1, cpus))


CARGO_BUILD_JOBS = _build_jobs()


# Validate host architecture
ARCH = platform.machine()
if ARCH == "x86_64":
    DEFAULT_CARGO_TARGET = "x86_64-unknown-linux-gnu"
    DEFAULT_CARGO_MUSL_TARGET = "x86_64-unknown-linux-musl"
elif ARCH == "aarch64":
    DEFAULT_CARGO_TARGET = "aarch64-unknown-linux-gnu"
    DEFAULT_CARGO_MUSL_TARGET = "aarch64-unknown-linux-musl"
else:
    raise ValueError(f"Unsupported host platform: {ARCH}")


@object_type
class Veilid:

    def _base_container(self) -> dagger.Container:
        """Creates the base container with all environment variables and initial setup"""

        return (
            dag.container()
            .from_(LINUX_BUILD_IMAGE)
            .with_env_variable("RUSTUP_HOME", RUSTUP_HOME)
            .with_env_variable("RUSTUP_DIST_SERVER", RUSTUP_DIST_SERVER)
            .with_env_variable("CARGO_HOME", CARGO_HOME)
            .with_env_variable("CARGO_INCREMENTAL", CARGO_INCREMENTAL)
            .with_env_variable("CARGO_PROFILE_TEST_DEBUGINFO", CARGO_PROFILE_TEST_DEBUGINFO)
            .with_env_variable("CARGO_BUILD_JOBS", CARGO_BUILD_JOBS)
            .with_env_variable("PATH", f"$PATH:{CARGO_HOME}/bin:/usr/local/zig", expand=True)
            .with_env_variable("LD_LIBRARY_PATH", "/usr/local/lib")
            .with_env_variable("RUST_BACKTRACE", "1")
            .with_env_variable("RETRY_COUNT", RETRY_COUNT)
            .with_env_variable("BINSTALL_DISABLE_TELEMETRY", "true")
            .with_env_variable("BINSTALL_NO_CONFIRM", "true")
            .with_env_variable("DEFAULT_CARGO_TARGET", DEFAULT_CARGO_TARGET)
            .with_workdir("/veilid")
        )

    @function
    def deps_base(self) -> dagger.Container:
        """Install build prerequisites & setup required directories"""
        container = self._base_container()

        # Configure apt
        apt_config = f"""Acquire::Retries "{RETRY_COUNT}";
        Acquire::https::Timeout "240";
        Acquire::http::Timeout "240";
        APT::Get::Assume-Yes "true";
        APT::Install-Recommends "false";
        APT::Install-Suggests "false";
        Debug::Acquire::https "true";"""

        container = container.with_new_file("/etc/apt/apt.conf.d/99custom", apt_config)
        container = container.with_new_file(
            "/usr/local/bin/retry", RETRY_SCRIPT, permissions=0o755
        )

        # Update package lists
        container = container.with_exec(["apt-get", "-y", "update"])

        # Install base packages including Windows cross-compilation tools
        base_packages = [
            "apt-get",
            "install",
            "-y",
            "ca-certificates",
            "iproute2",
            "curl",
            "build-essential",
            "libssl-dev",
            "openssl",
            "file",
            "git",
            "pkg-config",
            "libdbus-1-dev",
            "libdbus-glib-1-dev",
            "libgirepository1.0-dev",
            "libcairo2-dev",
            "checkinstall",
            "unzip",
            "zip",
            "libncursesw5-dev",
            "libncurses5-dev",
            "gcc-mingw-w64-x86-64",
            "mingw-w64",
            "jq",
            "selinux-policy-dev",
            "bzip2",
        ]
        container = container.with_exec(base_packages)

        # Install cross-compilation toolchains and development libraries based on architecture
        arch = platform.machine()
        if arch == "x86_64":
            container = container.with_exec(
                ["apt-get", "install", "-y", "gcc-aarch64-linux-gnu", "libc6-dev-arm64-cross"]
            )
        elif arch == "aarch64":
            container = container.with_exec(
                [
                    "apt-get",
                    "install",
                    "-y",
                    "gcc-x86-64-linux-gnu",
                    "libc6-dev-amd64-cross",
                    "gcc-multilib-x86-64-linux-gnu",
                ]
            )
        else:
            container = container.with_exec(
                [
                    "apt-get",
                    "install",
                    "-y",
                    "gcc-aarch64-linux-gnu",
                    "gcc-x86-64-linux-gnu",
                    "libc6-dev-arm64-cross",
                    "libc6-dev-amd64-cross",
                    "gcc-multilib-x86-64-linux-gnu",
                ]
            )

        return container

    @function
    def tool_rust(self) -> dagger.Container:
        """Rust toolchain: pinned compiler, clippy, every target in versions.env, test nightly.

        One tool per function, assembled by deps_rust. The engine caches each chain on its
        own inputs, so bumping ZIG_VERSION no longer rebuilds ast-grep and friends. The
        Earthly build splits the same way, with each tool kept as its own registry image.
        """
        container = self.deps_base()

        # Install rustup and Rust toolchain
        container = container.with_exec(
            [
                "sh",
                "-c",
                f"curl --retry {RETRY_COUNT} --retry-connrefused --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --default-toolchain={RUST_VERSION} -y -c clippy --no-modify-path --profile minimal",
            ]
        )

        # Set permissions and verify installation
        container = (
            container.with_exec(["chmod", "-R", "a+w", RUSTUP_HOME, CARGO_HOME])
            .with_exec(["rustup", "--version"])
            .with_exec(["cargo", "--version"])
            .with_exec(["rustc", "--version"])
        )

        # Rust targets: single source of truth is versions.env (RUST_STABLE_TARGETS /
        # RUST_NIGHTLY_TARGETS, loaded at module top). No Apple desktop/iOS targets —
        # Dagger is Linux-only and builds no Apple binaries (clippy uses the
        # aarch64-apple-darwin already in the shared stable set).

        # Add a single default-target nightly toolchain for some tests
        container = container.with_exec(
            [
                "retry",
                "rustup",
                "toolchain",
                "install",
                f"{RUST_UNIT_TESTS_NIGHTLY_VERSION}",
                "-c",
                "miri,rust-src",
            ]
        )

        # Add targets
        # No --toolchain: targets must land on the DEFAULT (pinned RUST_VERSION)
        # toolchain the builds use. `--toolchain stable` makes rustup auto-install a
        # separate "stable" toolchain and attach the targets THERE — every cross-target
        # build then fails E0463 "can't find crate for core". (Matches the Earthfile.)
        container = container.with_exec(["retry", "rustup", "target", "add", *RUST_STABLE_TARGETS])

        # Add nightly targets
        container = container.with_exec(
            ["retry", "rustup", "target", "add", "--toolchain", "nightly", *RUST_NIGHTLY_TARGETS]
        )

        return container

    @function
    def tool_cargo_bins(self) -> dagger.Directory:
        """Cargo helpers, installed under /tools with --root so the result is just these
        binaries and a version bump here leaves the rust toolchain cached."""
        # Install cargo tools (try binstall of musl targets first, then fallback to regular install)
        # (Doing it this way so we don't have to install the musl rust targets, which binstall will default to building with)
        container = (
            self.tool_rust()
            .with_exec(["mkdir", "-p", "/tools/bin"])
            .with_exec(
                [
                    # cargo-binstall itself, which the wasm-bindgen-cli install also needs
                    "retry",
                    "bash",
                    "-c",
                    f"set -o pipefail; curl --retry {RETRY_COUNT} --retry-connrefused -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash",
                ]
            )
            .with_exec(["cp", f"{CARGO_HOME}/bin/cargo-binstall", "/tools/bin/"])
        )
        for crate, version in (
            ("cargo-msrv", CARGO_MSRV_VERSION),
            ("cargo-nextest", CARGO_NEXTEST_VERSION),
            ("cargo-docs-rs", CARGO_DOCS_RS_VERSION),
            ("cargo-public-api", CARGO_PUBLIC_API_VERSION),
            ("cargo-sync-readme", CARGO_SYNC_README_VERSION),
        ):
            container = container.with_exec(
                [
                    "sh",
                    "-c",
                    f"cargo binstall --root /tools --version {version} {crate} --disable-strategies=compile --targets {DEFAULT_CARGO_MUSL_TARGET} || "
                    f"cargo install --root /tools --version {version} {crate} --locked",
                ]
            )
        return container.directory("/tools/bin")

    @function
    def tool_astgrep(self) -> dagger.Directory:
        """Census gate engine. Built from source deliberately: ast-grep ships no musl
        artifact and its linux-gnu release targets a newer glibc than this base image,
        so binstall would fetch a binary that cannot run here."""
        return (
            self.tool_rust()
            .with_exec(
                [
                    "cargo",
                    "install",
                    "--root",
                    "/tools",
                    "--version",
                    AST_GREP_VERSION,
                    "ast-grep",
                    "--locked",
                ]
            )
            .directory("/tools/bin")
        )

    @function
    def tool_zigbuild(self) -> dagger.Directory:
        """cargo-zigbuild. A source build; no binstall available for zigbuild yet."""
        return (
            self.tool_rust()
            .with_exec(
                [
                    "cargo",
                    "install",
                    "--root",
                    "/tools",
                    "--version",
                    CARGO_ZIGBUILD_VERSION,
                    "cargo-zigbuild",
                    "--locked",
                ]
            )
            .directory("/tools/bin")
        )

    @function
    def tool_zig(self) -> dagger.Directory:
        """Zig, for cross-compilation"""
        arch = platform.machine()
        zig_url = (
            f"https://ziglang.org/download/{ZIG_VERSION}/zig-linux-{arch}-{ZIG_VERSION}.tar.xz"
        )
        return (
            self.deps_base()
            .with_exec(
                [
                    "retry",
                    "curl",
                    "--retry",
                    RETRY_COUNT,
                    "--retry-connrefused",
                    "-L",
                    "-O",
                    zig_url,
                ]
            )
            .with_exec(
                ["tar", "-C", "/usr/local", "-xJf", f"zig-linux-{arch}-{ZIG_VERSION}.tar.xz"]
            )
            .with_exec(["mv", f"/usr/local/zig-linux-{arch}-{ZIG_VERSION}", "/usr/local/zig"])
            .directory("/usr/local/zig")
        )

    @function
    def tool_binaryen(self) -> dagger.Directory:
        """wasm-opt"""
        arch = platform.machine()
        binaryen_url = f"https://github.com/WebAssembly/binaryen/releases/download/version_{BINARYEN_VERSION}/binaryen-version_{BINARYEN_VERSION}-{arch}-linux.tar.gz"
        return (
            self.deps_base()
            .with_exec(
                [
                    "retry",
                    "curl",
                    "--retry",
                    RETRY_COUNT,
                    "--retry-connrefused",
                    "-L",
                    "-o",
                    "binaryen.tar.gz",
                    "-O",
                    binaryen_url,
                ]
            )
            .with_exec(["mkdir", "-p", "/tmp/binaryen", "/tools/bin"])
            .with_exec(
                ["tar", "-C", "/tmp/binaryen", "-xvf", "binaryen.tar.gz", "--strip-components=1"]
            )
            .with_exec(["cp", "/tmp/binaryen/bin/wasm-opt", "/tools/bin/"])
            .directory("/tools/bin")
        )

    @function
    def deps_rust(self) -> dagger.Container:
        """Every cached tool assembled into the toolchain the builds start from"""
        rust = self.tool_rust()
        # Merged onto the toolchain's own bin first, so the result is right whether the
        # container copy below merges into the destination or replaces it.
        cargo_bin = (
            rust.directory(f"{CARGO_HOME}/bin")
            .with_directory(".", self.tool_cargo_bins())
            .with_directory(".", self.tool_astgrep())
            .with_directory(".", self.tool_zigbuild())
            .with_directory(".", self.tool_binaryen())
        )
        return (
            self.deps_base()
            .with_directory(RUSTUP_HOME, rust.directory(RUSTUP_HOME))
            .with_directory(CARGO_HOME, rust.directory(CARGO_HOME))
            .with_directory(f"{CARGO_HOME}/bin", cargo_bin)
            .with_directory("/usr/local/zig", self.tool_zig())
            # A missing tool here would be a silent copy, so prove the assembly runs
            .with_exec(
                [
                    "sh",
                    "-c",
                    "rustup --version && cargo --version && rustc --version && cargo-zigbuild --version"
                    " && ast-grep --version && cargo nextest --version && cargo binstall -V && wasm-opt --version",
                ]
            )
        )

    @function
    def deps_linux(self) -> dagger.Container:
        """Linux build dependencies (base + rust without Android tools)"""
        # In Dagger, we can simply call deps_rust which already includes everything needed
        # The Earthfile version copies artifacts, but in Dagger the container already has everything
        return self.deps_rust()

    @function
    def deps_flutter(
        self, source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)]
    ) -> dagger.Container:
        """Flutter SDK for the veilid-flutter dart format and integration gates.

        Newer base than the rest of the build: these gates ship nothing, so the
        LINUX_BUILD_IMAGE glibc floor that keeps the release binaries portable buys them
        nothing, and Flutter's own floor is well above it. No rust here: the format gate
        does not need it, and code_flutter_rust adds it for the gates that do.
        """
        # Flutter publishes a linux-x64 SDK only, so this half of the build is amd64-only
        if ARCH != "x86_64":
            raise ValueError(
                "the flutter gates need an x86_64 host; Flutter ships no linux-arm64 SDK"
            )

        flutter_sdk_dir = "/usr/local/share/veilid-flutter-sdk"
        container = (
            dag.container()
            .from_(FLUTTER_BUILD_IMAGE)
            .with_env_variable("DEBIAN_FRONTEND", "noninteractive")
            .with_env_variable("RUSTUP_HOME", RUSTUP_HOME)
            .with_env_variable("CARGO_HOME", CARGO_HOME)
            .with_env_variable("FLUTTER_SDK_DIR", flutter_sdk_dir)
            .with_env_variable(
                "PATH", f"$PATH:{CARGO_HOME}/bin:{flutter_sdk_dir}/flutter/bin", expand=True
            )
            .with_env_variable("CARGO_INCREMENTAL", CARGO_INCREMENTAL)
            .with_env_variable("CARGO_BUILD_JOBS", CARGO_BUILD_JOBS)
            .with_env_variable("RUST_BACKTRACE", "1")
            .with_exec(["apt-get", "-y", "update"])
            .with_exec(
                [
                    "apt-get",
                    "install",
                    "-y",
                    "ca-certificates",
                    "curl",
                    "git",
                    "unzip",
                    "xz-utils",
                    "file",
                    "build-essential",
                    "pkg-config",
                    "libssl-dev",
                ]
            )
        )

        # The same pinned installer the dev-setup scripts use, so CI and a dev host land on one SDK
        container = (
            container.with_new_file("/tmp/flutter-install/versions.env", _VERSIONS_ENV_TEXT)
            .with_file(
                "/tmp/flutter-install/_flutter_install.sh",
                source.file("dev-setup/_flutter_install.sh"),
            )
            .with_exec(
                [
                    "sh",
                    "-c",
                    ". /tmp/flutter-install/versions.env && "
                    ". /tmp/flutter-install/_flutter_install.sh && i_flutter",
                ]
            )
            # flutter shells out to git against its own checkout, which this container did not clone
            .with_exec(
                [
                    "git",
                    "config",
                    "--global",
                    "--add",
                    "safe.directory",
                    f"{flutter_sdk_dir}/flutter",
                ]
            )
            .with_exec(["flutter", "--version"])
        )

        return container

    @function
    def code_flutter(
        self, source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)]
    ) -> dagger.Container:
        """Import the veilid code repository on top of the Flutter SDK"""
        return (
            self.deps_flutter(source)
            .with_directory("/veilid", source)
            # DIRECTORY_IGNORE_PATTERNS drops .dagger from the upload, and the dart
            # source walk and the integration script both source versions.env from there
            .with_new_file("/veilid/.dagger/versions.env", _VERSIONS_ENV_TEXT)
            .with_workdir("/veilid")
        )

    @function
    def code_flutter_rust(
        self, source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)]
    ) -> dagger.Container:
        """The flutter gates that build veilid-flutter's native library need a rust toolchain
        on top. The dart format gate does not, and stays on the lighter code_flutter."""
        return (
            self.deps_flutter_rust(source)
            .with_directory("/veilid", source)
            .with_new_file("/veilid/.dagger/versions.env", _VERSIONS_ENV_TEXT)
            .with_workdir("/veilid")
        )

    @function
    def deps_flutter_rust(
        self, source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)]
    ) -> dagger.Container:
        """deps_flutter plus the rust toolchain, mounted BEFORE any source.

        Ordering is the point: mounting a 1.7GB toolchain after the source (which is what
        code_flutter_rust used to do) puts it downstream of every source change and makes it
        uncacheable. The Earthly path publishes this as deps-flutter-rust for the same reason.
        """
        rust = self.tool_rust()
        return (
            self.deps_flutter(source)
            .with_directory(RUSTUP_HOME, rust.directory(RUSTUP_HOME))
            .with_directory(CARGO_HOME, rust.directory(CARGO_HOME))
            .with_exec(["sh", "-c", "cargo --version && rustc --version"])
        )

    @function
    def fetch_cache(
        self, source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)]
    ) -> dagger.Container:
        """Pre-download every crate in Cargo.lock so no later job depends on reaching crates.io.

        Manifests and the lockfile only. `cargo fetch` resolves from Cargo.lock, which records
        the whole dependency graph regardless of features, so one fetch covers every feature set
        the test jobs use — including the optional trees (geolocation, rt-async-std) that a
        per-feature dependency prebuild would have had to enumerate by hand.

        The glob keeps new workspace members included automatically; the */smoketest manifests it
        also matches declare their own [workspace] and are ignored. .cargo/config.toml comes along
        because cargo reads it while resolving.
        """
        container = self.deps_linux().with_directory(
            "/veilid",
            source,
            include=["**/Cargo.toml", "Cargo.lock", ".cargo/config.toml"],
        )
        # All one exec: deleting registry/src in a later layer would leave its bulk in this one.
        # Order matters twice over — the first delete forces the --offline re-fetch to work from
        # the .crate files alone, which is what proves the cache is complete (an image short a
        # crate fails silently downstream), and that re-fetch re-extracts src, so it has to be
        # deleted again. src is re-extracted locally with no network, and dropping it saves ~6x.
        return container.with_exec(
            [
                "sh",
                "-c",
                f"cargo fetch --locked && rm -rf {CARGO_HOME}/registry/src"
                f" && cargo fetch --locked --offline && rm -rf {CARGO_HOME}/registry/src",
            ]
        )

    def _crate_cache_image(self, ci_registry_image: str, crate_cache_tag: str) -> dagger.Container:
        """The published crate download cache (see publish_crate_cache)."""
        return dag.container().from_(f"{ci_registry_image}/crate-cache:{crate_cache_tag}")

    @function
    def code_linux(
        self,
        source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)],
        ci_registry_image: str = VEILID_REPO,
        crate_cache_tag: str = "",
    ) -> dagger.Container:
        """Import the whole veilid code repository with build dependencies"""
        # Crates arrive pre-downloaded when crate_cache_tag names a fetch_cache image; otherwise
        # the build downloads them itself. The cache lives in CARGO_HOME, outside /veilid, so the
        # source mount below cannot disturb it — with_directory REPLACES its mount path where
        # Earthly's COPY merges, and keeping the cache clear of that path is what lets both
        # builders share one code path with no artifact to capture and re-inject.
        if crate_cache_tag:
            container = self._crate_cache_image(ci_registry_image, crate_cache_tag)
        else:
            container = self.deps_linux()

        container = container.with_directory("/veilid", source).with_workdir("/veilid")

        # Check to make sure Cargo.lock is up to date
        container = container.with_exec(["cargo", "update", "-w", "--locked"])

        # Restore original Cargo.lock (copy it again to ensure it's preserved)
        container = container.with_file("Cargo.lock", source.file("Cargo.lock"))

        # Install the wasm-bindgen CLI tool that we need, which depends on the Cargo.lock version of wasm-bindgen being used
        container = container.with_exec(
            [
                "sh",
                "-c",
                f"WASM_BINDGEN_VERSION=$(cargo tree --locked -p veilid-wasm -i wasm-bindgen | head -n 1 | cut -c 15-); "
                f"cargo binstall wasm-bindgen-cli --disable-strategies=compile --targets {DEFAULT_CARGO_MUSL_TARGET} --version $WASM_BINDGEN_VERSION || "
                f"cargo install wasm-bindgen-cli --locked --version $WASM_BINDGEN_VERSION",
            ]
        )

        return container

    @function
    async def publish_deps_base(
        self,
        ci_registry_image: str = VEILID_REPO,
        deps_base_tag: str = "latest",
        registry_user: str = "",
        registry_password: dagger.Secret | None = None,
    ) -> str:
        """Publish the OS layer as deps-base:<deps_base_tag> (Earthly's +deps-base-build).

        Dagger does not need this for its own caching — the engine is content-addressed and
        rebuilds nothing — but the Earthly path publishes it so that parallel CI jobs pull the
        apt+cmake layer instead of each rebuilding it, and so everything layered above dedupes
        against blobs already in the registry. Kept here so both builders can populate the same
        images; scripts/tool_tags.sh deps-base is the tag.
        """
        c = self.deps_base()
        if registry_password is not None:
            address = ci_registry_image.split("/", 1)[0]
            c = c.with_registry_auth(address, registry_user, registry_password)
        return await c.publish(f"{ci_registry_image}/deps-base:{deps_base_tag}")

    @function
    async def publish_deps_linux(
        self,
        ci_registry_image: str = VEILID_REPO,
        deps_linux_tag: str = "latest",
        registry_user: str = "",
        registry_password: dagger.Secret | None = None,
    ) -> str:
        """Publish the assembled toolchain as deps-linux:<deps_linux_tag> (Earthly's
        +deps-linux-build). See publish_deps_base for why the Earthly path needs these and
        Dagger does not; scripts/tool_tags.sh deps-linux is the tag.
        """
        c = self.deps_linux()
        if registry_password is not None:
            address = ci_registry_image.split("/", 1)[0]
            c = c.with_registry_auth(address, registry_user, registry_password)
        return await c.publish(f"{ci_registry_image}/deps-linux:{deps_linux_tag}")

    @function
    async def publish_deps_flutter(
        self,
        source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)],
        ci_registry_image: str = VEILID_REPO,
        deps_flutter_tag: str = "latest",
        registry_user: str = "",
        registry_password: dagger.Secret | None = None,
    ) -> str:
        """Publish the flutter gates' base as deps-flutter:<tag> (Earthly's +deps-flutter-build).
        See publish_deps_base for why the Earthly path needs these images and Dagger does not.
        scripts/tool_tags.sh deps-flutter is the tag."""
        c = self.deps_flutter(source)
        if registry_password is not None:
            address = ci_registry_image.split("/", 1)[0]
            c = c.with_registry_auth(address, registry_user, registry_password)
        return await c.publish(f"{ci_registry_image}/deps-flutter:{deps_flutter_tag}")

    @function
    async def publish_deps_flutter_rust(
        self,
        source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)],
        ci_registry_image: str = VEILID_REPO,
        deps_flutter_rust_tag: str = "latest",
        registry_user: str = "",
        registry_password: dagger.Secret | None = None,
    ) -> str:
        """Publish deps_flutter plus the rust toolchain as deps-flutter-rust:<tag>
        (Earthly's +deps-flutter-rust-build). scripts/tool_tags.sh deps-flutter-rust is the tag."""
        c = self.deps_flutter_rust(source)
        if registry_password is not None:
            address = ci_registry_image.split("/", 1)[0]
            c = c.with_registry_auth(address, registry_user, registry_password)
        return await c.publish(f"{ci_registry_image}/deps-flutter-rust:{deps_flutter_rust_tag}")

    @function
    async def publish_crate_cache(
        self,
        source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)],
        ci_registry_image: str = VEILID_REPO,
        crate_cache_tag: str = "latest",
        registry_user: str = "",
        registry_password: dagger.Secret | None = None,
    ) -> str:
        """Publish the crate download cache as crate-cache:<crate_cache_tag>
        (the Dagger equivalent of Earthly's +fetch-cache SAVE IMAGE --push).

        fetch_cache already asserts completeness with an --offline re-fetch, so what lands here
        is a toolchain image plus every crate in Cargo.lock. It carries nothing compiled and
        nothing arch-specific, so any consumer on any architecture gets the same benefit.
        """
        c = self.fetch_cache(source)
        if registry_password is not None:
            address = ci_registry_image.split("/", 1)[0]
            c = c.with_registry_auth(address, registry_user, registry_password)
        return await c.publish(f"{ci_registry_image}/crate-cache:{crate_cache_tag}")

    @function
    async def lint(
        self,
        source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)],
        ci_registry_image: str = VEILID_REPO,
        crate_cache_tag: str = "",
    ) -> str:
        """Run source linting for multiple targets"""
        container = self.code_linux(source, ci_registry_image, crate_cache_tag)

        # Run clippy for different targets
        result = await container.with_exec(
            ["./scripts/_lint_all.sh", "--cleanup"]
        ).combined_output()

        return f"Clippy completed:\n{result}"

    @function
    async def smoketest(
        self,
        source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)],
        ci_registry_image: str = VEILID_REPO,
        crate_cache_tag: str = "",
    ) -> str:
        """Run Smoketest checks for all published crates"""
        container = self.code_linux(source, ci_registry_image, crate_cache_tag)

        result = await container.with_exec(
            ["./scripts/_smoketest_all.sh", "--locked"]
        ).combined_output()

        return f"MSRV check completed:\n{result}"

    @function
    async def msrv(
        self,
        source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)],
        ci_registry_image: str = VEILID_REPO,
        crate_cache_tag: str = "",
    ) -> str:
        """Run MSRV checks for all crates"""
        container = self.code_linux(source, ci_registry_image, crate_cache_tag)

        result = await container.with_exec(["./scripts/_msrv_all.sh"]).combined_output()

        return f"MSRV check completed:\n{result}"

    @function
    async def public_api(
        self,
        source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)],
        ci_registry_image: str = VEILID_REPO,
        crate_cache_tag: str = "",
    ) -> str:
        """Run public API checks for all crates"""
        container = self.code_linux(source, ci_registry_image, crate_cache_tag)

        result = await container.with_exec(["./scripts/_check_public_api.sh"]).combined_output()

        return f"Public API check completed:\n{result}"

    @function
    async def census(
        self,
        source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)],
        ci_registry_image: str = VEILID_REPO,
        crate_cache_tag: str = "",
    ) -> str:
        """Run the census gates: the rule self-test, then the inventory drift check"""
        container = self.code_linux(source, ci_registry_image, crate_cache_tag)

        result = await container.with_exec(
            [
                "sh",
                "-c",
                "./scripts/test_census.sh && CENSUS_CHECK_ONLY=1 ./scripts/update_census.sh",
            ]
        ).combined_output()

        return f"Census gate completed:\n{result}"

    @function
    async def dart_format(
        self, source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)]
    ) -> str:
        """Run the Dart format gate over veilid-flutter"""
        container = self.code_flutter(source)

        result = await container.with_exec(["./scripts/_check_format_dart.sh"]).combined_output()

        return f"Dart format gate completed:\n{result}"

    @function
    async def flutter_integration(
        self, source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)]
    ) -> str:
        """Run the veilid-flutter integration tests against the in-process mock, headless"""
        container = self.code_flutter_rust(source)

        result = await container.with_exec(
            ["./scripts/_flutter_integration_tests.sh"]
        ).combined_output()

        return f"Flutter integration tests completed:\n{result}"

    @function
    def build_windows_amd64(
        self,
        source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)],
        ci_registry_image: str = VEILID_REPO,
        crate_cache_tag: str = "",
    ) -> dagger.Directory:
        """Build Windows AMD64 binaries and return the target directory"""
        container = self.code_linux(source, ci_registry_image, crate_cache_tag)
        target = "x86_64-pc-windows-gnu"

        # Build release binaries for x86_64-pc-windows-gnu
        # Careful not to include veilid-flutter or veilid-wasm here as they add the 'json-camel-case' feature
        # which will screw up the build of veilid-server because it will automatically add that feature inappropriately
        out = (
            container.with_exec(
                [
                    "cargo",
                    "zigbuild",
                    "--locked",
                    "--target",
                    target,
                    "--release",
                    "-p",
                    "veilid-server",
                    "-p",
                    "veilid-cli",
                    "-p",
                    "veilid-tools",
                    "-p",
                    "veilid-core-optimized",
                    "-p",
                    "veilid-core",
                    "-p",
                    "veilid-remote-api",
                ]
            )
            .with_exec(["rm", "-rf", f"./target/{target}/release/.fingerprint"])
            .with_exec(["rm", "-rf", f"./target/{target}/release/build"])
            .with_exec(["rm", "-rf", f"./target/{target}/release/deps"])
            .with_exec(["rm", "-rf", f"./target/{target}/release/examples"])
            .with_exec(["rm", "-rf", f"./target/{target}/release/incremental"])
            .directory(f"./target/{target}/release")
        )

        # Return the built artifacts directory
        return out

    @function
    def build_linux_amd64(
        self,
        source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)],
        ci_registry_image: str = VEILID_REPO,
        crate_cache_tag: str = "",
    ) -> dagger.Directory:
        """Build Linux AMD64 binaries and return the target directory"""
        container = self.code_linux(source, ci_registry_image, crate_cache_tag)
        target = "x86_64-unknown-linux-gnu"
        # cargo-zigbuild reads the `.<glibc>` suffix and links against zig's stubs for that
        # release; cargo still sees the bare triple, so target/ paths below are unaffected.
        build_target = f"{target}.{GLIBC_FLOOR}"

        # Build release binaries for x86_64-unknown-linux-gnu
        # Careful not to include veilid-flutter or veilid-wasm here as they add the 'json-camel-case' feature
        # which will screw up the build of veilid-server because it will automatically add that feature inappropriately
        out = (
            container.with_exec(
                [
                    "cargo",
                    "zigbuild",
                    "--locked",
                    "--target",
                    build_target,
                    "--release",
                    "-p",
                    "veilid-server",
                    "-p",
                    "veilid-cli",
                    "-p",
                    "veilid-tools",
                    "-p",
                    "veilid-core-optimized",
                    "-p",
                    "veilid-core",
                    "-p",
                    "veilid-remote-api",
                ]
            )
            .with_exec(["rm", "-rf", f"./target/{target}/release/.fingerprint"])
            .with_exec(["rm", "-rf", f"./target/{target}/release/build"])
            .with_exec(["rm", "-rf", f"./target/{target}/release/deps"])
            .with_exec(["rm", "-rf", f"./target/{target}/release/examples"])
            .with_exec(["rm", "-rf", f"./target/{target}/release/incremental"])
            .directory(f"./target/{target}/release")
        )

        # Return the built artifacts directory
        return out

    @function
    def build_linux_arm64(
        self,
        source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)],
        ci_registry_image: str = VEILID_REPO,
        crate_cache_tag: str = "",
    ) -> dagger.Directory:
        """Build Linux ARM64 binaries and return the target directory"""
        container = self.code_linux(source, ci_registry_image, crate_cache_tag)
        target = "aarch64-unknown-linux-gnu"
        # cargo-zigbuild reads the `.<glibc>` suffix and links against zig's stubs for that
        # release; cargo still sees the bare triple, so target/ paths below are unaffected.
        build_target = f"{target}.{GLIBC_FLOOR}"

        # Build release binaries for aarch64-unknown-linux-gnu
        # Careful not to include veilid-flutter or veilid-wasm here as they add the 'json-camel-case' feature
        # which will screw up the build of veilid-server because it will automatically add that feature inappropriately
        out = (
            container.with_exec(
                [
                    "cargo",
                    "zigbuild",
                    "--locked",
                    "--target",
                    build_target,
                    "--release",
                    "-p",
                    "veilid-server",
                    "-p",
                    "veilid-cli",
                    "-p",
                    "veilid-tools",
                    "-p",
                    "veilid-core-optimized",
                    "-p",
                    "veilid-core",
                    "-p",
                    "veilid-remote-api",
                ]
            )
            .with_exec(["rm", "-rf", f"./target/{target}/release/.fingerprint"])
            .with_exec(["rm", "-rf", f"./target/{target}/release/build"])
            .with_exec(["rm", "-rf", f"./target/{target}/release/deps"])
            .with_exec(["rm", "-rf", f"./target/{target}/release/examples"])
            .with_exec(["rm", "-rf", f"./target/{target}/release/incremental"])
            .directory(f"./target/{target}/release")
        )

        # Return the built artifacts directory
        return out

    # No support yet. One could do this with a host-mount of the Apple Developer SDKs and running on a MacOS machine.
    # @function
    # def build_macos_arm64(self, source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)], ci_registry_image: str = VEILID_REPO, crate_cache_tag: str = "") -> dagger.Directory:
    #     """Build MacOS ARM64 binaries and return the target directory"""
    #     container = self.code_linux(source, ci_registry_image, crate_cache_tag)
    #     target="aarch64-apple-darwin"
    #     # Build release binaries for aarch64-apple-darwin
    #     # Careful not to include veilid-flutter or veilid-wasm here as they add the 'json-camel-case' feature
    #     # which will screw up the build of veilid-server because it will automatically add that feature inappropriately
    #     out = (
    #         container.with_exec([
    #             "cargo", "zigbuild", "--locked", "--target", target, "--release", "-p", "veilid-server", "-p", "veilid-cli", "-p", "veilid-tools", "-p", "veilid-core-optimized", "-p", "veilid-core", "-p", "veilid-remote-api"
    #         ])
    #         .with_exec(["rm", "-rf", f"./target/{target}/release/.fingerprint"])
    #         .with_exec(["rm", "-rf", f"./target/{target}/release/build"])
    #         .with_exec(["rm", "-rf", f"./target/{target}/release/deps"])
    #         .with_exec(["rm", "-rf", f"./target/{target}/release/examples"])
    #         .with_exec(["rm", "-rf", f"./target/{target}/release/incremental"])
    #         .directory(f"./target/{target}/release")
    #     )

    #     # Return the built artifacts directory
    #     return out

    @function
    async def test_msrv_all(
        self,
        source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)],
        ci_registry_image: str = VEILID_REPO,
        crate_cache_tag: str = "",
    ) -> str:
        """Run MSRV checks for all crates"""
        container = self.code_linux(source, ci_registry_image, crate_cache_tag)
        result = await container.with_exec(["./scripts/_msrv_all.sh"]).combined_output()
        return f"MSRV check completed:\n{result}"

    @function
    async def test_public_api(
        self,
        source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)],
        ci_registry_image: str = VEILID_REPO,
        crate_cache_tag: str = "",
    ) -> str:
        """Run public API checks for all crates"""
        container = self.code_linux(source, ci_registry_image, crate_cache_tag)
        result = await container.with_exec(["./scripts/_check_public_api.sh"]).combined_output()
        return f"Public API check completed:\n{result}"

    @function
    async def test_census(
        self,
        source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)],
        ci_registry_image: str = VEILID_REPO,
        crate_cache_tag: str = "",
    ) -> str:
        """Run the census gates"""
        return await self.census(source, ci_registry_image, crate_cache_tag)

    @function
    async def test_dart_format(
        self, source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)]
    ) -> str:
        """Run the Dart format gate"""
        return await self.dart_format(source)

    @function
    async def test_flutter_integration(
        self, source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)]
    ) -> str:
        """Run the veilid-flutter integration tests"""
        return await self.flutter_integration(source)

    @function
    async def test_readme_sync(
        self,
        source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)],
        ci_registry_image: str = VEILID_REPO,
        crate_cache_tag: str = "",
    ) -> str:
        """Check that every generated crate README matches its crate documentation"""
        container = self.code_linux(source, ci_registry_image, crate_cache_tag)
        result = await container.with_exec(["./scripts/check_readme_sync.sh"]).combined_output()
        return f"README sync check completed:\n{result}"

    @function
    async def test_smoketest_all(
        self,
        source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)],
        ci_registry_image: str = VEILID_REPO,
        crate_cache_tag: str = "",
    ) -> str:
        """Run Smoketest checks for all published crates"""
        container = self.code_linux(source, ci_registry_image, crate_cache_tag)
        result = await container.with_exec(
            ["./scripts/_smoketest_all.sh", "--locked"]
        ).combined_output()
        return f"Smoketest check completed:\n{result}"

    @function
    async def test_check_wasm_builds_dart(
        self,
        source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)],
        ci_registry_image: str = VEILID_REPO,
        crate_cache_tag: str = "",
    ) -> str:
        """Check WASM builds for Dart"""
        container = self.code_linux(source, ci_registry_image, crate_cache_tag)
        result = await container.with_exec(
            ["./scripts/_check_wasm_builds.sh", "dart"]
        ).combined_output()
        return f"WASM Dart build check completed:\n{result}"

    @function
    async def test_check_wasm_builds_js(
        self,
        source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)],
        ci_registry_image: str = VEILID_REPO,
        crate_cache_tag: str = "",
    ) -> str:
        """Check WASM builds for JS"""
        container = self.code_linux(source, ci_registry_image, crate_cache_tag)
        result = await container.with_exec(
            ["./scripts/_check_wasm_builds.sh", "js"]
        ).combined_output()
        return f"WASM JS build check completed:\n{result}"

    @function
    async def test_clippy_wasm32_unknown_unknown(
        self,
        source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)],
        ci_registry_image: str = VEILID_REPO,
        crate_cache_tag: str = "",
    ) -> str:
        """Run clippy for wasm32-unknown-unknown"""
        container = self.code_linux(source, ci_registry_image, crate_cache_tag)
        result = await container.with_exec(
            ["./scripts/_clippy_wasm32_unknown_unknown.sh"]
        ).combined_output()
        return f"Clippy wasm32-unknown-unknown completed:\n{result}"

    @function
    async def test_clippy_x86_64_pc_windows_gnu(
        self,
        source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)],
        ci_registry_image: str = VEILID_REPO,
        crate_cache_tag: str = "",
    ) -> str:
        """Run clippy for x86_64-pc-windows-gnu"""
        container = self.code_linux(source, ci_registry_image, crate_cache_tag)
        result = await container.with_exec(
            ["./scripts/_clippy_x86_64_pc_windows_gnu.sh"]
        ).combined_output()
        return f"Clippy x86_64-pc-windows-gnu completed:\n{result}"

    @function
    async def test_clippy_aarch64_apple_darwin(
        self,
        source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)],
        ci_registry_image: str = VEILID_REPO,
        crate_cache_tag: str = "",
    ) -> str:
        """Run clippy for aarch64-apple-darwin"""
        container = self.code_linux(source, ci_registry_image, crate_cache_tag)
        result = await container.with_exec(
            ["./scripts/_clippy_aarch64_apple_darwin.sh"]
        ).combined_output()
        return f"Clippy aarch64-apple-darwin completed:\n{result}"

    @function
    async def test_clippy_x86_64_unknown_linux_gnu(
        self,
        source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)],
        ci_registry_image: str = VEILID_REPO,
        crate_cache_tag: str = "",
    ) -> str:
        """Run clippy for x86_64-unknown-linux-gnu"""
        container = self.code_linux(source, ci_registry_image, crate_cache_tag)
        result = await container.with_exec(
            ["./scripts/_clippy_x86_64_unknown_linux_gnu.sh"]
        ).combined_output()
        return f"Clippy x86_64-unknown-linux-gnu completed:\n{result}"

    @function
    async def test_clippy_x86_64_unknown_linux_gnu_async_std(
        self,
        source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)],
        ci_registry_image: str = VEILID_REPO,
        crate_cache_tag: str = "",
    ) -> str:
        """Run clippy for x86_64-unknown-linux-gnu with async-std"""
        container = self.code_linux(source, ci_registry_image, crate_cache_tag)
        result = await container.with_exec(
            ["./scripts/_clippy_x86_64_unknown_linux_gnu_async_std.sh"]
        ).combined_output()
        return f"Clippy x86_64-unknown-linux-gnu async-std completed:\n{result}"

    @function
    async def test_clippy_mock_api(
        self,
        source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)],
        ci_registry_image: str = VEILID_REPO,
        crate_cache_tag: str = "",
    ) -> str:
        """Run clippy for the mock-api configuration"""
        container = self.code_linux(source, ci_registry_image, crate_cache_tag)
        result = await container.with_exec(["./scripts/_clippy_mock_api.sh"]).combined_output()
        return f"Clippy mock-api completed:\n{result}"

    @function
    async def test_clippy_optional_features(
        self,
        source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)],
        ci_registry_image: str = VEILID_REPO,
        crate_cache_tag: str = "",
    ) -> str:
        """Run clippy with the optional observability features (verbose-tracing, geolocation, aimd-dht, stats)"""
        container = self.code_linux(source, ci_registry_image, crate_cache_tag)
        result = await container.with_exec(
            ["./scripts/_clippy_optional_features.sh"]
        ).combined_output()
        return f"Clippy optional-features completed:\n{result}"

    @function
    async def test_build_docs(
        self,
        source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)],
        ci_registry_image: str = VEILID_REPO,
        crate_cache_tag: str = "",
    ) -> str:
        """Build documentation"""
        container = self.code_linux(source, ci_registry_image, crate_cache_tag)
        result = await container.with_exec(["./scripts/_build_docs.sh"]).combined_output()
        return f"Documentation build completed:\n{result}"

    @function
    async def test_unit_tests_all(
        self,
        source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)],
        ci_registry_image: str = VEILID_REPO,
        crate_cache_tag: str = "",
    ) -> str:
        """Run all unit tests"""
        container = self.code_linux(source, ci_registry_image, crate_cache_tag)
        result = await container.with_exec(["./scripts/_unit_tests_all.sh"]).combined_output()
        return f"Unit tests completed:\n{result}"

    @function
    async def test_all(
        self,
        source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)],
        ci_registry_image: str = VEILID_REPO,
        crate_cache_tag: str = "",
    ) -> str:
        """Run all tests (serially! for desktop, not CI use!)"""
        results = []
        results.append(await self.test_census(source, ci_registry_image, crate_cache_tag))
        results.append(await self.test_dart_format(source))
        results.append(await self.test_msrv_all(source, ci_registry_image, crate_cache_tag))
        results.append(await self.test_public_api(source, ci_registry_image, crate_cache_tag))
        results.append(await self.test_readme_sync(source, ci_registry_image, crate_cache_tag))
        results.append(await self.test_smoketest_all(source, ci_registry_image, crate_cache_tag))
        results.append(
            await self.test_check_wasm_builds_dart(source, ci_registry_image, crate_cache_tag)
        )
        results.append(
            await self.test_check_wasm_builds_js(source, ci_registry_image, crate_cache_tag)
        )
        results.append(
            await self.test_clippy_wasm32_unknown_unknown(
                source, ci_registry_image, crate_cache_tag
            )
        )
        results.append(
            await self.test_clippy_x86_64_pc_windows_gnu(
                source, ci_registry_image, crate_cache_tag
            )
        )
        results.append(
            await self.test_clippy_aarch64_apple_darwin(source, ci_registry_image, crate_cache_tag)
        )
        results.append(
            await self.test_clippy_x86_64_unknown_linux_gnu(
                source, ci_registry_image, crate_cache_tag
            )
        )
        results.append(
            await self.test_clippy_x86_64_unknown_linux_gnu_async_std(
                source, ci_registry_image, crate_cache_tag
            )
        )
        results.append(await self.test_clippy_mock_api(source, ci_registry_image, crate_cache_tag))
        results.append(
            await self.test_clippy_optional_features(source, ci_registry_image, crate_cache_tag)
        )
        results.append(await self.test_build_docs(source, ci_registry_image, crate_cache_tag))
        results.append(await self.test_unit_tests_all(source, ci_registry_image, crate_cache_tag))
        results.append(await self.test_flutter_integration(source))
        return "\n\n=== TEST SUMMARY ===\n" + "\n\n".join(results)

    @function
    async def test_docs(
        self,
        source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)],
        rust_nightly_version: str,
        ci_registry_image: str = VEILID_REPO,
        crate_cache_tag: str = "",
    ) -> str:
        """Build and test documentation with a specific nightly version (for package testing)"""
        container = self.code_linux(source, ci_registry_image, crate_cache_tag)
        result = await container.with_exec(
            ["./scripts/_build_docs.sh", rust_nightly_version]
        ).combined_output()
        return f"Documentation build completed:\n{result}"

    @function
    async def package_test_all(
        self,
        source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)],
        is_nightly: bool = False,
        ci_registry_image: str = VEILID_REPO,
        crate_cache_tag: str = "",
    ) -> str:
        """Tests to run before packaging releases"""
        results = []

        # Test DOCS.RS build with most recent nightly (this will probably install a newer version than what is in the cache, but for this test it is important to use the latest nightly)
        docs_result = await self.test_docs(
            source, RUST_PACKAGE_TESTS_NIGHTLY_VERSION, ci_registry_image, crate_cache_tag
        )
        results.append(docs_result)

        return "\n\n=== TEST SUMMARY ===\n" + "\n\n".join(results)

    @function
    def selinux_policy_deb(
        self, source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)]
    ) -> dagger.File:
        """The .deb's SELinux policy module, compiled on its own pinned image.

        checkmodule emits the newest policy version its refpolicy knows (18.04 -> 19, 22.04 -> 20,
        24.04 -> 21) and a module will not load on a system whose libsepol is older, so this floor
        is a support decision (SELINUX_POLICY_IMAGE) rather than a side effect of whatever the
        compiler runs on. The RPM path builds its own module on RPM_BUILD_IMAGE.
        """
        return (
            dag.container()
            .from_(SELINUX_POLICY_IMAGE)
            .with_env_variable("DEBIAN_FRONTEND", "noninteractive")
            .with_exec(["apt-get", "-y", "update"])
            .with_exec(["apt-get", "install", "-y", "selinux-policy-dev", "bzip2", "make"])
            .with_directory("/work/selinux", source.directory("package/selinux"))
            .with_workdir("/work/selinux")
            .with_exec(["./build_module.sh"])
            .file("/work/selinux/veilid_server.pp.bz2")
        )

    @function
    def package_deb(
        self,
        source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)],
        target_arch: str,
        is_nightly: bool = False,
        ci_registry_image: str = VEILID_REPO,
        crate_cache_tag: str = "",
    ) -> dagger.Directory:
        """Package DEB files for specified architecture"""
        # Get built binaries
        if target_arch == "amd64":
            build_dir = self.build_linux_amd64(source, ci_registry_image, crate_cache_tag)
            rust_target = "x86_64-unknown-linux-gnu"
        elif target_arch == "arm64":
            build_dir = self.build_linux_arm64(source, ci_registry_image, crate_cache_tag)
            rust_target = "aarch64-unknown-linux-gnu"
        else:
            raise ValueError(f"Unsupported architecture: {target_arch}")

        # Start with code-linux for the packaging scripts
        container = self.code_linux(source, ci_registry_image, crate_cache_tag)

        # Copy build artifacts into container
        container = (
            container.with_directory("/veilid/package", source.directory("package"))
            .with_file(
                f"/veilid/target/{rust_target}/release/veilid-server",
                build_dir.file("veilid-server"),
            )
            .with_file(
                f"/veilid/target/{rust_target}/release/veilid-cli", build_dir.file("veilid-cli")
            )
        )

        # Built on SELINUX_POLICY_IMAGE, not here: see selinux_policy_deb.
        container = container.with_file(
            "/veilid/package/selinux/veilid_server.pp.bz2", self.selinux_policy_deb(source)
        )

        # Set nightly flag
        nightly_flag = "true" if is_nightly else "false"

        # Create DEB packages
        container = container.with_exec(
            [
                "/veilid/package/debian/earthly_make_veilid_server_deb.sh",
                target_arch,
                rust_target,
                nightly_flag,
                GLIBC_FLOOR,
            ]
        ).with_exec(
            [
                "/veilid/package/debian/earthly_make_veilid_cli_deb.sh",
                target_arch,
                rust_target,
                nightly_flag,
                GLIBC_FLOOR,
            ]
        )

        # Return the package output directory
        return container.directory("/dpkg/out")

    @function
    def package_rpm(
        self,
        source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)],
        target_arch: str,
        is_nightly: bool = False,
        ci_registry_image: str = VEILID_REPO,
        crate_cache_tag: str = "",
    ) -> dagger.Directory:
        """Package RPM files for specified architecture"""
        # Get built binaries
        if target_arch == "x86_64":
            build_dir = self.build_linux_amd64(source, ci_registry_image, crate_cache_tag)
            rust_target = "x86_64-unknown-linux-gnu"
            platform = "linux/amd64"
        elif target_arch == "aarch64":
            build_dir = self.build_linux_arm64(source, ci_registry_image, crate_cache_tag)
            rust_target = "aarch64-unknown-linux-gnu"
            platform = "linux/arm64"
        else:
            raise ValueError(f"Unsupported architecture: {target_arch}")

        # Use Rocky Linux for RPM packaging
        container = (
            dag.container(platform=dagger.Platform(platform))
            .from_(RPM_BUILD_IMAGE)
            .with_exec(
                [
                    "yum",
                    "install",
                    "-y",
                    "createrepo",
                    "rpm-build",
                    "rpm-sign",
                    "yum-utils",
                    "rpmdevtools",
                    "selinux-policy-devel",
                    "bzip2",
                    "make",
                ]
            )
            .with_exec(["rpmdev-setuptree"])
        )

        # Set up directory structure
        container = (
            container.with_exec(["mkdir", "-p", "/veilid/target"])
            .with_exec(["mkdir", "-p", "/veilid/veilid-cli", "/veilid/veilid-server"])
            .with_exec(["mkdir", "-p", "/rpm-work-dir/veilid-server"])
        )

        # Copy necessary files
        container = (
            container.with_file(
                "/veilid/veilid-cli/Cargo.toml", source.file("veilid-cli/Cargo.toml")
            )
            .with_file("/veilid/veilid-server/Cargo.toml", source.file("veilid-server/Cargo.toml"))
            .with_directory("/veilid/package", source.directory("package"))
            .with_file(
                f"/veilid/target/{rust_target}/release/veilid-server",
                build_dir.file("veilid-server"),
            )
            .with_file(
                f"/veilid/target/{rust_target}/release/veilid-cli", build_dir.file("veilid-cli")
            )
        )

        # Compile the SELinux policy module in-place (RHEL-family refpolicy)
        container = container.with_exec(
            ["bash", "-c", "cd /veilid/package/selinux && ./build_module.sh"]
        )

        # Set nightly flag
        nightly_flag = "true" if is_nightly else "false"

        # Create RPM packages
        container = container.with_exec(
            [
                "veilid/package/rpm/veilid-server/earthly_make_veilid_server_rpm.sh",
                target_arch,
                rust_target,
                nightly_flag,
            ]
        ).with_exec(
            [
                "veilid/package/rpm/veilid-cli/earthly_make_veilid_cli_rpm.sh",
                target_arch,
                rust_target,
                nightly_flag,
            ]
        )

        # Return the RPM output directory
        return container.directory(f"/root/rpmbuild/RPMS/{target_arch}")

    @function
    async def package_zip(
        self,
        source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)],
        target_arch: str,
        is_nightly: bool = False,
        ci_registry_image: str = VEILID_REPO,
        crate_cache_tag: str = "",
    ) -> dagger.Directory:
        """Package ZIP files for specified architecture"""
        # Get built binaries
        if target_arch == "amd64":
            build_dir = self.build_windows_amd64(source, ci_registry_image, crate_cache_tag)
            rust_target = "x86_64-pc-windows-gnu"
        # elif target_arch == "arm64":
        #     build_dir = self.build_windows_arm64(source, ci_registry_image, crate_cache_tag)
        #     rust_target = "aarch64-pc-windows-gnullvm"
        else:
            raise ValueError(f"Unsupported architecture: {target_arch}")

        # Start with code-linux for the packaging scripts
        container = self.code_linux(source, ci_registry_image, crate_cache_tag)

        # Copy build artifacts into container
        container = container.with_directory(f"/veilid/target/{rust_target}/release", build_dir)

        # Copy package directory
        container = container.with_directory("/veilid/package", source.directory("package"))

        # Create ZIP packages
        container = (
            container.with_workdir(f"/veilid/target/{rust_target}/release")
            .with_exec(["mkdir", "-p", "/out"])
            .with_exec(
                ["/veilid/package/cargo_version.sh", "/veilid/veilid-server/Cargo.toml"],
                redirect_stdout="/tmp/version",
            )
            .with_exec(["sh", "-c", r"date '+%Y%m%d'"], redirect_stdout="/tmp/datestamp")
        )
        version = (await container.file("/tmp/version").contents()).strip()
        datestamp = (await container.file("/tmp/datestamp").contents()).strip()

        if is_nightly:
            container = container.with_exec(
                [
                    "zip",
                    f"/out/veilid-{datestamp}_{target_arch}.zip",
                    "veilid-server.exe",
                    "veilid-cli.exe",
                ]
            )
        else:
            container = container.with_exec(
                [
                    "zip",
                    f"/out/veilid-{version}_{target_arch}.zip",
                    "veilid-server.exe",
                    "veilid-cli.exe",
                ]
            )

        # Return the package output directory
        return container.directory("/out")

    @function
    def package_linux_amd64(
        self,
        source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)],
        is_nightly: bool = False,
        ci_registry_image: str = VEILID_REPO,
        crate_cache_tag: str = "",
    ) -> dagger.Directory:
        """Package both DEB and RPM for Linux AMD64"""
        # Create a container to collect all packages
        container = dag.container().from_(PACKAGING_IMAGE).with_exec(["mkdir", "-p", "/packages"])

        # Get DEB packages
        deb_dir = self.package_deb(source, "amd64", is_nightly, ci_registry_image, crate_cache_tag)
        container = container.with_directory("/packages/deb", deb_dir)

        # Get RPM packages
        rpm_dir = self.package_rpm(
            source, "x86_64", is_nightly, ci_registry_image, crate_cache_tag
        )
        container = container.with_directory("/packages/rpm", rpm_dir)

        return container.directory("/packages")

    @function
    def package_linux_arm64(
        self,
        source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)],
        is_nightly: bool = False,
        ci_registry_image: str = VEILID_REPO,
        crate_cache_tag: str = "",
    ) -> dagger.Directory:
        """Package both DEB and RPM for Linux ARM64"""
        # Create a container to collect all packages
        container = dag.container().from_(PACKAGING_IMAGE).with_exec(["mkdir", "-p", "/packages"])

        # Get DEB packages
        deb_dir = self.package_deb(source, "arm64", is_nightly, ci_registry_image, crate_cache_tag)
        container = container.with_directory("/packages/deb", deb_dir)

        # Get RPM packages
        rpm_dir = self.package_rpm(
            source, "aarch64", is_nightly, ci_registry_image, crate_cache_tag
        )
        container = container.with_directory("/packages/rpm", rpm_dir)

        return container.directory("/packages")

    @function
    async def package_linux(
        self,
        source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)],
        is_nightly: bool = False,
        ci_registry_image: str = VEILID_REPO,
        crate_cache_tag: str = "",
    ) -> dagger.Directory:
        """Package for all Linux architectures (AMD64 and ARM64)"""

        # The docs.rs build gates the release: a crate that will not build there is not
        # publishable, and finding that out after the packages are cut is too late
        await self.test_docs(
            source, RUST_PACKAGE_TESTS_NIGHTLY_VERSION, ci_registry_image, crate_cache_tag
        )

        # Create a container to collect all packages
        container = dag.container().from_(PACKAGING_IMAGE).with_exec(["mkdir", "-p", "/packages"])

        # Get AMD64 packages
        amd64_dir = self.package_linux_amd64(
            source, is_nightly, ci_registry_image, crate_cache_tag
        )
        container = container.with_directory("/packages/amd64", amd64_dir)

        # Get ARM64 packages
        arm64_dir = self.package_linux_arm64(
            source, is_nightly, ci_registry_image, crate_cache_tag
        )
        container = container.with_directory("/packages/arm64", arm64_dir)

        return container.directory("/packages")

    @function
    async def package_windows_amd64(
        self,
        source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)],
        is_nightly: bool = False,
        ci_registry_image: str = VEILID_REPO,
        crate_cache_tag: str = "",
    ) -> dagger.Directory:
        """Package ZIP for Windows AMD64"""
        # Create a container to collect all packages
        container = dag.container().from_(PACKAGING_IMAGE).with_exec(["mkdir", "-p", "/packages"])

        # Get Zip packages
        zip_dir = await self.package_zip(
            source, "amd64", is_nightly, ci_registry_image, crate_cache_tag
        )
        container = container.with_directory("/packages/zip", zip_dir)

        return container.directory("/packages")

    @function
    async def package_windows(
        self,
        source: Annotated[dagger.Directory, Ignore(DIRECTORY_IGNORE_PATTERNS)],
        is_nightly: bool = False,
        ci_registry_image: str = VEILID_REPO,
        crate_cache_tag: str = "",
    ) -> dagger.Directory:
        """Package for all Windows architectures (AMD64)"""
        # Create a container to collect all packages
        container = dag.container().from_(PACKAGING_IMAGE).with_exec(["mkdir", "-p", "/packages"])

        # Get AMD64 packages
        amd64_dir = await self.package_windows_amd64(
            source, is_nightly, ci_registry_image, crate_cache_tag
        )
        container = container.with_directory("/packages/amd64", amd64_dir)

        return container.directory("/packages")
