#!/bin/bash
# veilid Linux dev-setup — one idempotent script. By default it CHECKS every
# development dependency and reports what's missing or out of date. With --install
# it installs the missing ones and upgrades the outdated ones (apt or dnf). A dep
# pinned to an exact version that would need a *downgrade* (and can't be installed
# side-by-side) is reported as a conflict to uninstall manually.
#
# Usage: ./setup_linux.sh [--install] [--android]
# Standalone: this script has no dependency on veilidchat. Supports Debian/Ubuntu/
# Mint (apt) and Fedora (dnf).
set -uo pipefail
SCRIPTDIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
source "$SCRIPTDIR/_dep.sh"

usage() {
    cat <<EOF
veilid Linux dev-setup — checks development dependencies; installs them with --install.
Supports Debian/Ubuntu/Mint (apt) and Fedora (dnf).

Usage: $0 [--install | --upgrade] [--android] [-h|--help]
  (default)            check each dependency and report what is missing or out of date
  --install            install missing deps + bring below-minimum deps up to spec
                       (leaves deps that already meet the requirement untouched)
  --upgrade, --update  also upgrade already-satisfied deps to the newest versions
  --build-only         only build-critical deps (skip cargo-nextest/public-api/wasm tools, nightly)
  --prefix <dir>       contained build (flutter-packer): install rust into the toolchain prefix
                       (RUSTUP_HOME/CARGO_HOME set by the caller); never apt/dnf — reuse host system libs
  --android            also check (and install/upgrade) the Android SDK: NDK, build-tools, ...
  --flutter            also install Flutter (for veilid-flutter work); only if missing or older than
                       veilid-flutter requires — never downgrades a newer one
  -h, --help           show this help
EOF
}
dep_help_requested "$@" && { usage; exit 0; }

[ "$(uname)" = Linux ] || { echo "Not running on Linux"; exit 1; }
# Root is normal in CI containers (sudo becomes a no-op, and may not exist there);
# only refuse root on an interactive dev box.
if [ "$(id -u)" -eq 0 ]; then
    if [ -n "${CI:-}" ] || [ -f /.dockerenv ] || [ -f /run/.containerenv ]; then
        sudo() { "$@"; }
    else
        echo "Don't run this as root (it sudos when needed)"; exit 1
    fi
fi

if command -v apt-get >/dev/null 2>&1; then PKG=apt
elif command -v dnf >/dev/null 2>&1; then PKG=dnf
else echo "Unsupported Linux: need apt or dnf"; exit 1; fi

ANDROID=0; FLUTTER=0
for a in "$@"; do [ "$a" = "--android" ] && ANDROID=1; [ "$a" = "--flutter" ] && FLUTTER=1; done
dep_parse_args "$@"

# --prefix <dir> / FLP_TOOLCHAIN_PREFIX: contained toolchain (flutter-packer). Signals "no global
# package install" — the caller points RUSTUP_HOME/CARGO_HOME into <dir> and dev-setup installs rust
# there; the apt/dnf-backed deps (system libs, cmake, clang, jq, java) are reused from the host (it
# must already have them, like Xcode on macOS), so apt/dnf is never invoked. Prepend the prefix cargo
# bin to PATH so this script's own rustup/cargo calls use the contained toolchain.
PREFIX="${FLP_TOOLCHAIN_PREFIX:-}"; _pp=""
for a in "$@"; do
    case "$_pp" in --prefix) PREFIX="$a" ;; esac
    case "$a" in --prefix=*) PREFIX="${a#--prefix=}" ;; esac
    _pp="$a"
done
# Self-sufficient prefix env (a caller's exports win): rustup-init installs
# wherever CARGO_HOME/RUSTUP_HOME point, and the checks look in the prefix —
# without these, --prefix without caller exports installs to ~/.cargo while
# the check looks in $PREFIX/cargo (permanent mismatch).
if [ -n "$PREFIX" ]; then
    export CARGO_HOME="${CARGO_HOME:-$PREFIX/cargo}"
    export RUSTUP_HOME="${RUSTUP_HOME:-$PREFIX/rustup}"
    export PATH="$CARGO_HOME/bin:$PATH"
fi

# ---- versions: single source of truth (.dagger/versions.env) ---------------
source "$SCRIPTDIR/../.dagger/versions.env"
source "$SCRIPTDIR/_flutter_install.sh"   # i_flutter / flutter_ver (optional --flutter; shared with veilidchat)
REQ_RUST=$RUST_MSRV;            REQ_JAVA=$JAVA_MIN_VERSION;   REQ_CAPNP=$CAPNP_MIN_VERSION
REQ_CMAKE=$CMAKE_MIN_VERSION;   REQ_WASMOPT=$BINARYEN_VERSION
REQ_WASMBINDGEN=$WASM_BINDGEN_VERSION; REQ_CARGOEDIT=$CARGO_EDIT_VERSION; REQ_NDK=$ANDROID_NDK_VERSION
# android rust targets, derived from the single-source RUST_STABLE_TARGETS (no duplicated list)
RUST_ANDROID_TARGETS=""; for _t in $RUST_STABLE_TARGETS; do case "$_t" in *android*) RUST_ANDROID_TARGETS="$RUST_ANDROID_TARGETS $_t" ;; esac; done

# ---- OS package install helpers --------------------------------------------
# apt and dnf names for the C/build libraries veilid-core + veilid-flutter need.
# C/build libraries veilid-core + veilid-flutter need. capnp (a veilid-core dev tool that
# regenerates checked-in capnp Rust) is NOT bundled here — it's the `capnp` dep below, gated so
# --build-only can skip it. (protobuf is veilidchat's, installed by its own setup.)
APT_LIBS="build-essential pkg-config libssl-dev openssl libdbus-1-dev libdbus-glib-1-dev \
  libgirepository1.0-dev libcairo2-dev libgtk-3-dev liblzma-dev libsecret-1-0 \
  clang llvm file git curl unzip checkinstall jq python3-pip openjdk-$JAVA_VERSION-jdk-headless"
DNF_LIBS="gcc-c++ make pkg-config openssl-devel openssl dbus-devel dbus-glib gobject-introspection-devel \
  cairo-devel gtk3-devel xz-devel libsecret clang llvm file git curl unzip \
  jq python3-pip java-$JAVA_VERSION-openjdk-headless"
# Under --prefix (contained build) never apt/dnf — the system libs/tools are reused from the host
# (it must already have them, like Xcode on macOS); only rust is installed, into the prefix.
pkg_install() { [ -n "$PREFIX" ] && return 0
                if [ "$PKG" = apt ]; then sudo apt-get update -y && sudo apt-get install -y $APT_LIBS
                else sudo dnf install -y $DNF_LIBS; sudo dnf groupinstall -y 'Development Tools'; fi; }
# apt/dnf install fetches the repo-latest (so it both installs and upgrades).
pkgi()    { [ -n "$PREFIX" ] && return 0; if [ "$PKG" = apt ]; then sudo apt-get install -y "$@"; else sudo dnf install -y "$@"; fi; }
i_libs()  { pkg_install; }
i_rust()  { if command -v rustup >/dev/null 2>&1; then rustup update; else curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y -c clippy --profile default; . "${CARGO_HOME:-$HOME/.cargo}/env"; fi; }
# --prefix: self-contained rustup-init into CARGO_HOME (proxies in CARGO_HOME/bin), pinned to
# RUST_VERSION + set default. RUSTUP_INIT_SKIP_PATH_CHECK=yes forces it past a host rustup on PATH.
i_prust() { curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | RUSTUP_INIT_SKIP_PATH_CHECK=yes sh -s -- -y --no-modify-path --profile minimal --default-toolchain "$RUST_VERSION"; }
i_capnp() { pkgi capnproto; }
i_cmake() { pkgi cmake; }   # distro cmake; for a newer cmake use cmake.org or snap
i_clang() { pkgi clang llvm; }
i_jq()    { pkgi jq; }
i_java()  { if [ "$PKG" = apt ]; then pkgi openjdk-$JAVA_VERSION-jdk-headless; else pkgi java-$JAVA_VERSION-openjdk-headless; fi; }
i_pip()   { pkgi python3 python3-pip; }
# wasm-opt: download the pinned binaryen; no-op if the installed version already
# satisfies the requirement (avoids a downgrade under --upgrade).
i_wopt()  { ver_ge "$(ver_of "$(wasm-opt --version 2>/dev/null)")" "$BINARYEN_VERSION" 2>/dev/null && return 0
            local a; a=$(uname -m); curl -L -o /tmp/binaryen.tar.gz \
              "https://github.com/WebAssembly/binaryen/releases/download/version_$BINARYEN_VERSION/binaryen-version_$BINARYEN_VERSION-$a-linux.tar.gz" \
              && mkdir -p /tmp/binaryen && tar -C /tmp/binaryen -xf /tmp/binaryen.tar.gz --strip-components=1 \
              && cp /tmp/binaryen/bin/wasm-opt "${CARGO_HOME:-$HOME/.cargo}/bin/"; }
i_wbg()   { cargo install --locked wasm-bindgen-cli --version "$REQ_WASMBINDGEN"; }
i_cedit() { cargo install --locked cargo-edit --version "$REQ_CARGOEDIT"; }
i_nxt()   { cargo install --locked cargo-nextest; }
i_papi()  { cargo install --locked cargo-public-api; }
crate_ver() { cargo install --list 2>/dev/null | sed -n "s/^$1 v\([0-9.]*\):.*/\1/p" | head -1; }

echo "== veilid Linux dev dependencies ($PKG) =="

# Rust toolchain. Under --prefix, check/install a self-contained rust in CARGO_HOME (pinned to
# RUST_VERSION, set default) by its explicit path — independent of any host rustc on PATH. Else the
# normal min-version check against the host toolchain.
if [ -n "$PREFIX" ]; then
    PCARGOBIN="${CARGO_HOME:-$PREFIX/cargo}/bin"
    dep_have "rust (prefix)"  "$("$PCARGOBIN/rustc" --version 2>/dev/null)" "rustup-init $RUST_VERSION into \$CARGO_HOME" i_prust
    dep_have "cargo (prefix)" "$([ -x "$PCARGOBIN/cargo" ] && echo y)"     "comes with the contained rustup"
else
    dep_min   "rust"   "$(rustc --version 2>/dev/null)" "$REQ_RUST" i_rust
    dep_have  "cargo"  "$(command -v cargo)"            "comes with rustup"
fi
# Dev/test cargo tools — skipped under --build-only (not needed to build the app).
if [ "$BUILDONLY" != 1 ]; then
    dep_exact "wasm-bindgen-cli"    "$(crate_ver wasm-bindgen-cli)"    "$REQ_WASMBINDGEN" 1 i_wbg
    dep_exact "cargo-edit"          "$(crate_ver cargo-edit)"          "$REQ_CARGOEDIT"   1 i_cedit
    dep_have  "cargo-nextest"       "$(crate_ver cargo-nextest)"       "cargo install --locked cargo-nextest"    i_nxt
    dep_have  "cargo-public-api"    "$(crate_ver cargo-public-api)"    "cargo install --locked cargo-public-api" i_papi
fi

# Native build tools + libraries.
dep_have "build libs"  "$(pkg-config --exists gtk+-3.0 dbus-1 2>/dev/null && echo y)" "$PKG build/gtk/dbus/ssl dev libs" i_libs
dep_min  "java"        "$(java -version 2>&1)"             "$REQ_JAVA"    i_java
# capnp: veilid-core dev tool (regenerates checked-in capnp Rust); skip under --build-only.
[ "$BUILDONLY" = 1 ] || dep_min "capnp" "$(capnp --version 2>/dev/null)" "$REQ_CAPNP" i_capnp
dep_min  "cmake"       "$(cmake --version 2>/dev/null)"    "$REQ_CMAKE"   i_cmake
[ "$BUILDONLY" = 1 ] || dep_min "wasm-opt" "$(wasm-opt --version 2>/dev/null)" "$REQ_WASMOPT" i_wopt   # wasm-only
dep_have "llvm/clang"  "$(command -v clang)"               "$PKG install clang llvm" i_clang
dep_have "jq"          "$(command -v jq)"                  "$PKG install jq" i_jq
dep_have "python3"     "$(command -v python3)"             "$PKG install python3" i_pip
dep_have "pip3"        "$(command -v pip3)"                "$PKG install python3-pip" i_pip
dep_have "node"        "$(command -v node)"                "install from https://nodejs.org or your package manager"
dep_have "npm"         "$(command -v npm)"                 "comes with node"

# Rust targets. Full dev installs the shared cross set (no Apple — can't link on Linux). Under
# --build-only the set is driven by the *target* platform: --android → the android targets;
# otherwise just the host linux-gnu target (the desktop build target).
if [ "$BUILDONLY" = 1 ]; then
    if [ "$ANDROID" = 1 ]; then dep_targets $RUST_ANDROID_TARGETS
    else case "$(uname -m)" in aarch64|arm64) _ha=aarch64 ;; *) _ha=x86_64 ;; esac; dep_targets "$_ha-unknown-linux-gnu"; fi
else
    dep_targets $RUST_STABLE_TARGETS
fi
# nightly toolchain: dev/test only (skipped under --build-only). Install if absent under
# --install; update only under --upgrade (it changes daily).
if [ "$BUILDONLY" != 1 ] && [ "$INSTALL" = 1 ] && command -v rustup >/dev/null 2>&1; then
    if ! rustup toolchain list 2>/dev/null | grep -q '^nightly'; then _run rustup install nightly --profile minimal
    elif [ "$UPGRADE" = 1 ]; then _run rustup update nightly; fi
fi

# Optional Android SDK (only with --android). NDK pinned exact (gradle path).
# A set-but-empty ANDROID_HOME is bootstrapped from scratch (the SDK is
# relocatable, no root needed) — CI points it into a cached prefix dir.
if [ "$ANDROID" = 1 ]; then
    echo "-- Android SDK --"
    if [ -n "${ANDROID_HOME:-}" ]; then
        SDKM="$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager"
        i_clt() {
            mkdir -p "$ANDROID_HOME/cmdline-tools" \
            && curl -fsSL -o "/tmp/clt-$$.zip" "https://dl.google.com/android/repository/commandlinetools-linux-${ANDROID_CMDLINE_TOOLS_VERSION}_latest.zip" \
            && rm -rf "$ANDROID_HOME/cmdline-tools/latest" \
            && unzip -q -o "/tmp/clt-$$.zip" -d "$ANDROID_HOME/cmdline-tools" \
            && rm -f "/tmp/clt-$$.zip" \
            && mv "$ANDROID_HOME/cmdline-tools/cmdline-tools" "$ANDROID_HOME/cmdline-tools/latest" \
            && sdk_accept_licenses
        }
        # yes|: a fresh SDK prompts for licenses; harmless on an accepted one. The
        # subshell drops pipefail so yes's SIGPIPE (sdkmanager closes the pipe first)
        # doesn't mask sdkmanager's real exit status.
        sdk_accept_licenses() { ( set +o pipefail; yes | "$SDKM" --licenses >/dev/null ); }
        i_ndk() { ( set +o pipefail; yes | "$SDKM" "ndk;$REQ_NDK" "build-tools;$ANDROID_BUILD_TOOLS_VERSION" "cmake;$ANDROID_CMAKE_VERSION" \
                  "platform-tools" "platforms;android-$ANDROID_PLATFORM_VERSION" ); }
        dep_have  "cmdline-tools" "$([ -x "$SDKM" ] && echo y)"                                  "bootstrap cmdline-tools into \$ANDROID_HOME" i_clt
        dep_exact "Android NDK"   "$([ -f "$ANDROID_HOME/ndk/$REQ_NDK/ndk-build" ] && echo "$REQ_NDK")" "$REQ_NDK" 1 i_ndk
        # gradle builds need no adb on PATH; the SDK's own copy satisfies the check
        dep_have  "adb"           "$([ -x "$ANDROID_HOME/platform-tools/adb" ] && echo y || command -v adb)" "add platform-tools to PATH"
    else
        dep_have "ANDROID_HOME" "" "install the Android SDK and export ANDROID_HOME (see install_linux_prerequisites notes)"
    fi
fi

# Optional Flutter SDK (only with --flutter) — for developers working on veilid-flutter. Installs the
# pinned FLUTTER_VERSION when flutter is absent or below veilid-flutter's requirement (FLUTTER_MIN_VERSION);
# dep_min never downgrades, so a newer flutter (e.g. one veilidchat installed) is left as-is.
if [ "$FLUTTER" = 1 ]; then
    echo "-- Flutter (veilid-flutter) --"
    dep_min  "flutter" "$(flutter_ver)"      "$FLUTTER_MIN_VERSION" i_flutter
    dep_have "dart"    "$(command -v dart)"  "comes with Flutter"
fi

dep_summary
