#!/bin/bash
# veilid macOS dev-setup — one idempotent script. By default it CHECKS every
# development dependency and reports what's missing or out of date. With --install
# it installs the missing ones and upgrades the outdated ones. A dep pinned to an
# exact version that would need a *downgrade* (and can't be installed side-by-side)
# is reported as a conflict for you to uninstall manually.
#
# Usage: ./setup_macos.sh [--install] [--android]
# Standalone: this script has no dependency on veilidchat.
set -uo pipefail
SCRIPTDIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
source "$SCRIPTDIR/_dep.sh"

usage() {
    cat <<EOF
veilid macOS dev-setup — checks development dependencies; installs them with --install.

Usage: $0 [--install | --upgrade] [--android] [-h|--help]
  (default)            check each dependency and report what is missing or out of date
  --install            install missing deps + bring below-minimum deps up to spec
                       (leaves deps that already meet the requirement untouched)
  --upgrade, --update  also upgrade already-satisfied deps to the newest versions
  --build-only         only build-critical deps (skip cargo-nextest/public-api/wasm tools, nightly)
  --prefix <dir>       contained build (flutter-packer): never install globally via brew; rust/dart
                       honor the caller's RUSTUP_HOME/CARGO_HOME/PUB_CACHE pointing into <dir>
  --android            also check (and install/upgrade) the Android SDK: NDK, build-tools, ...
  --flutter            also install Flutter (for veilid-flutter work); only if missing or older than
                       veilid-flutter requires — never downgrades a newer one
  -h, --help           show this help
EOF
}
dep_help_requested "$@" && { usage; exit 0; }

[ "$(uname)" = Darwin ] || { echo "Not running on macOS"; exit 1; }
[ "$(id -u)" -ne 0 ] || { echo "Don't run this as root"; exit 1; }

ANDROID=0; FLUTTER=0
for a in "$@"; do [ "$a" = "--android" ] && ANDROID=1; [ "$a" = "--flutter" ] && FLUTTER=1; done
dep_parse_args "$@"

# --prefix <dir> / FLP_TOOLCHAIN_PREFIX: contained toolchain (flutter-packer). Signals "no
# global brew" — the caller points RUSTUP_HOME/CARGO_HOME/PUB_CACHE into <dir> and dev-setup's
# env-honoring installers (rustup, dart pub) follow them; the brew-backed deps are reused from
# the host (Xcode) or owned by flutter-packer (cocoapods in GEM_HOME), so brew is never touched.
PREFIX="${FLP_TOOLCHAIN_PREFIX:-}"; _pp=""
for a in "$@"; do
    case "$_pp" in --prefix) PREFIX="$a" ;; esac
    case "$a" in --prefix=*) PREFIX="${a#--prefix=}" ;; esac
    _pp="$a"
done
# Under --prefix, put the prefix cargo bin first on PATH so this script's own rustup/cargo calls
# (the target installs) use the contained toolchain (flutter-packer also does this — harmless to repeat).
[ -n "$PREFIX" ] && export PATH="${CARGO_HOME:-$PREFIX/cargo}/bin:$PATH"

# ---- versions: single source of truth (.dagger/versions.env) ---------------
source "$SCRIPTDIR/../.dagger/versions.env"
source "$SCRIPTDIR/_flutter_install.sh"   # i_flutter / flutter_ver (optional --flutter; shared with veilidchat)
REQ_RUST=$RUST_MSRV;            REQ_JAVA=$JAVA_MIN_VERSION;   REQ_CAPNP=$CAPNP_MIN_VERSION
REQ_CMAKE=$CMAKE_MIN_VERSION;   REQ_WASMOPT=$BINARYEN_VERSION
REQ_WASMBINDGEN=$WASM_BINDGEN_VERSION; REQ_CARGOEDIT=$CARGO_EDIT_VERSION; REQ_NDK=$ANDROID_NDK_VERSION
# android rust targets, derived from the single-source RUST_STABLE_TARGETS (no duplicated list)
RUST_ANDROID_TARGETS=""; for _t in $RUST_STABLE_TARGETS; do case "$_t" in *android*) RUST_ANDROID_TARGETS="$RUST_ANDROID_TARGETS $_t" ;; esac; done

# ---- brew (non-interactive; honor BREW_USER for devs whose brew is another user's) ---
# NONINTERACTIVE: no prompts; HOMEBREW_NO_ENV_HINTS: drop the trailing hint spam. Set
# via `env` so they survive the sudo hop. Auto-update stays on so --upgrade sees newest.
BREW_ENV="NONINTERACTIVE=1 HOMEBREW_NO_ENV_HINTS=1"
if [ -z "${BREW_USER:-}" ]; then BREW_RUN="env $BREW_ENV brew"; else BREW_RUN="sudo -H -u $BREW_USER env $BREW_ENV brew"; fi
brew_run() { $BREW_RUN "$@"; }

# ---- per-dep installers (upgrade-safe: install if missing, upgrade if present) ----
# Under --prefix (contained build) never touch global brew — those deps are host-provided (Xcode)
# or flutter-packer-owned (cocoapods in the prefix GEM_HOME).
brew_ensure() { [ -n "$PREFIX" ] && return 0; if brew_run list "$1" >/dev/null 2>&1; then brew_run upgrade "$1" 2>/dev/null || true; else brew_run install "$1"; fi; }
i_clt()   { xcode-select --install; until [ -d /Library/Developer/CommandLineTools/usr/bin ]; do sleep 5; done; }
# rustup-init / `rustup update` honor RUSTUP_HOME/CARGO_HOME (the caller sets them under --prefix);
# source the cargo env from CARGO_HOME, not a hard-coded ~/.cargo.
i_rust()  { if command -v rustup >/dev/null 2>&1; then rustup update; else curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y -c clippy --profile default; . "${CARGO_HOME:-$HOME/.cargo}/env"; fi; }
# --prefix: self-contained rustup-init into CARGO_HOME — lays down cargo/rustc/rustup proxies in
# CARGO_HOME/bin, pinned to RUST_VERSION and set as default. RUSTUP_INIT_SKIP_PATH_CHECK=yes forces
# the install even when a host rustup is on PATH (a `rustup update` against a host install would
# never create the prefix proxies). --no-modify-path: don't touch shell rc; the caller owns PATH.
i_prust() { curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | RUSTUP_INIT_SKIP_PATH_CHECK=yes sh -s -- -y --no-modify-path --profile minimal --default-toolchain "$RUST_VERSION"; }
i_java()  { brew_ensure openjdk@$JAVA_VERSION; }
i_capnp() { brew_ensure capnp; }
i_cmake() { brew_ensure cmake; }
i_wopt()  { brew_ensure binaryen; }
i_llvm()  { brew_ensure llvm; }
i_jq()    { brew_ensure jq; }
i_node()  { brew_ensure node; }
i_pods()  { brew_ensure cocoapods; }
# --prefix: contained cocoapods into GEM_HOME (veilidchat has non-SPM macOS plugins, so it's needed).
# Needs a host ruby/gem (macOS ships one); no-op with a note if none, so dev-setup doesn't hard-fail.
i_pods_prefix() { command -v gem >/dev/null 2>&1 || { echo "    cocoapods: no ruby/gem on PATH — the runner must provide one"; return 0; }
                  local rv; rv=$(ruby -e 'print RUBY_VERSION' 2>/dev/null)
                  # cocoapods' ffi needs ruby >= 3.0; macOS *system* ruby is 2.6. Can't fix that contained
                  # (a newer ruby is a host/runner prereq) — skip with a clear note, don't hard-fail.
                  case "$rv" in 1.*|2.*) echo "    cocoapods: host ruby $rv too old (needs >= 3.0) — provide a newer ruby on the runner"; return 0 ;; esac
                  local gh="${GEM_HOME:-$PREFIX/gem}"; gem install --install-dir "$gh" --bindir "$gh/bin" --no-document cocoapods; }
i_wbg()   { cargo install --locked wasm-bindgen-cli --version "$REQ_WASMBINDGEN"; }
i_cedit() { cargo install --locked cargo-edit --version "$REQ_CARGOEDIT"; }
i_nxt()   { cargo install --locked cargo-nextest; }
i_papi()  { cargo install --locked cargo-public-api; }
crate_ver() { cargo install --list 2>/dev/null | sed -n "s/^$1 v\([0-9.]*\):.*/\1/p" | head -1; }

echo "== veilid macOS dev dependencies =="

# Toolchain prerequisites (manual installs — large IDE/package managers).
dep_have "Xcode"            "$(xcode-select -p 2>/dev/null)"                                "install Xcode from the App Store"
dep_have "CommandLineTools" "$([ -d /Library/Developer/CommandLineTools/usr/bin ] && echo y)" "run: xcode-select --install" i_clt
dep_have "Homebrew"         "$(command -v brew)"                                            "install from https://brew.sh"

# Rust toolchain. Under --prefix, check/install a *self-contained* rust in CARGO_HOME (pinned to
# RUST_VERSION, set as default) by its explicit path — independent of any host rustc on PATH. Else
# the normal min-version check against the host toolchain.
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
    dep_exact "wasm-bindgen-cli"    "$(crate_ver wasm-bindgen-cli)"      "$REQ_WASMBINDGEN" 1 i_wbg
    dep_exact "cargo-edit"          "$(crate_ver cargo-edit)"            "$REQ_CARGOEDIT"   1 i_cedit
    dep_have  "cargo-nextest"       "$(crate_ver cargo-nextest)"         "cargo install --locked cargo-nextest"   i_nxt
    dep_have  "cargo-public-api"    "$(crate_ver cargo-public-api)"      "cargo install --locked cargo-public-api" i_papi
fi

# Native build tools.
dep_min  "java"        "$(java -version 2>&1)"             "$REQ_JAVA"    i_java
# capnp only regenerates veilid-core's checked-in capnp Rust when a .capnp schema changes — a
# veilid-core *developer* tool, not a veilidchat build dep. Skip under --build-only.
[ "$BUILDONLY" = 1 ] || dep_min "capnp" "$(capnp --version 2>/dev/null)" "$REQ_CAPNP" i_capnp
# cmake: macOS/iOS build through Xcode, not cmake — under --build-only check only (assume host;
# no brew, and a missing/old cmake doesn't fail the run).
if [ "$BUILDONLY" = 1 ]; then dep_min "cmake" "$(cmake --version 2>/dev/null)" "$REQ_CMAKE"
else dep_min "cmake" "$(cmake --version 2>/dev/null)" "$REQ_CMAKE" i_cmake; fi
[ "$BUILDONLY" = 1 ] || dep_min "wasm-opt" "$(wasm-opt --version 2>/dev/null)" "$REQ_WASMOPT" i_wopt   # wasm-only
dep_have "llvm/clang"  "$(command -v clang)"               "brew install llvm" i_llvm
dep_have "jq"          "$(command -v jq)"                  "brew install jq"   i_jq
dep_have "python3"     "$(command -v python3)"             "comes with Xcode CLT / brew"
dep_have "node"        "$(command -v node)"                "brew install node" i_node
dep_have "npm"         "$(command -v npm)"                 "comes with node"
# cocoapods (veilidchat has non-SPM macOS plugins, so it's needed). Under --prefix install it
# CONTAINED into GEM_HOME; else brew (global). dev-setup owns the contained install — the gem/ruby
# mechanics live here (flutter-packer just sets GEM_HOME + does the SPM-presence decision upstream).
if [ -n "$PREFIX" ]; then
    dep_have "cocoapods (prefix)" "$([ -x "${GEM_HOME:-$PREFIX/gem}/bin/pod" ] && echo y)" "gem install cocoapods into \$GEM_HOME" i_pods_prefix
else
    dep_have "cocoapods" "$(command -v pod)" "brew install cocoapods" i_pods
fi

# Rust targets. Full dev installs the whole cross set. Under --build-only the set is driven by the
# *target* platform (like Windows -TargetArch), NOT the host OS: --android → the android targets;
# otherwise the apple targets (macOS/iOS). ~1 GB less rust-std than the full set either way.
if [ "$BUILDONLY" = 1 ]; then
    if [ "$ANDROID" = 1 ]; then dep_targets $RUST_ANDROID_TARGETS
    else dep_targets aarch64-apple-darwin $RUST_STABLE_TARGETS_APPLE; fi
else
    dep_targets $RUST_STABLE_TARGETS $RUST_STABLE_TARGETS_APPLE
fi
# nightly toolchain: dev/test only (skipped under --build-only). Install if absent under
# --install; update only under --upgrade (it changes daily).
if [ "$BUILDONLY" != 1 ] && [ "$INSTALL" = 1 ] && command -v rustup >/dev/null 2>&1; then
    if ! rustup toolchain list 2>/dev/null | grep -q '^nightly'; then _run rustup install nightly --profile minimal
    elif [ "$UPGRADE" = 1 ]; then _run rustup update nightly; fi
fi

# Optional Android SDK (only with --android). NDK is pinned exact (gradle path).
if [ "$ANDROID" = 1 ]; then
    echo "-- Android SDK --"
    if [ -n "${ANDROID_HOME:-}" ] && [ -d "$ANDROID_HOME" ]; then
        SDKM="$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager"
        i_ndk() { "$SDKM" "ndk;$REQ_NDK" "build-tools;$ANDROID_BUILD_TOOLS_VERSION" "cmake;$ANDROID_CMAKE_VERSION" "platform-tools" "platforms;android-$ANDROID_PLATFORM_VERSION"; }
        dep_have  "cmdline-tools" "$([ -x "$SDKM" ] && echo y)"                                  "install Android cmdline-tools"
        dep_exact "Android NDK"   "$([ -f "$ANDROID_HOME/ndk/$REQ_NDK/ndk-build" ] && echo "$REQ_NDK")" "$REQ_NDK" 1 i_ndk
        dep_have  "adb"           "$(command -v adb)"                                            "add platform-tools to PATH"
    else
        dep_have "ANDROID_HOME" "" "install Android Studio + SDK and export ANDROID_HOME"
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
