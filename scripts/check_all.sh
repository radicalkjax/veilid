#!/bin/bash
# Local pre-push gate: format check, lints, builds, docs and tests, in that order.
# Every command's status is checked explicitly and the first failure exits with that
# command's own exit code, so this is safe to wire up as a git pre-push hook.

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
if [ -z "$SCRIPTDIR" ]; then
    echo "check_all: could not resolve script directory" >&2
    exit 1
fi

fail() {
    echo "check_all: $*" >&2
    exit 1
}

# Run a command, announce it, and propagate its exit code on failure
step() {
    local rc
    echo "==> $*"
    "$@"
    rc=$?
    if [ $rc -ne 0 ]; then
        echo "check_all: FAILED (exit $rc): $*" >&2
        exit $rc
    fi
}

cd "$SCRIPTDIR/.." || fail "could not change to the repository root"

if [ $# -ge 1 ] && [ "$1" = "--cleanup" ] ; then
    CLEANUP="--cleanup"
    shift
else
    CLEANUP=""
fi

# Pin every sub-script's bare cargo/cargo-zigbuild to the CI toolchain so lints match CI.
# Explicit `+nightly` invocations (docs, nightly unit tests) still override this.
RUST_VERSION="$(. "$SCRIPTDIR/../.dagger/versions.env" && echo "$RUST_VERSION")"
if [ $? -ne 0 ] || [ -z "$RUST_VERSION" ]; then
    fail "could not read RUST_VERSION from .dagger/versions.env"
fi
if ! rustup toolchain list | grep -q "^${RUST_VERSION}-"; then
    echo "CI-pinned Rust toolchain $RUST_VERSION is not installed. Install it with:"
    echo "  rustup toolchain install $RUST_VERSION --profile minimal -c clippy -c rustfmt"
    exit 1
fi
export RUSTUP_TOOLCHAIN="$RUST_VERSION"
echo "Using CI-pinned Rust toolchain $RUST_VERSION."

# The lint and format steps need these components; a --profile minimal install has neither
INSTALLED_COMPONENTS="$(rustup component list --toolchain "$RUST_VERSION" --installed)"
if [ $? -ne 0 ]; then
    fail "could not list the installed Rust components for $RUST_VERSION"
fi
for cmp in clippy rustfmt; do
    if ! echo "$INSTALLED_COMPONENTS" | grep -q "^${cmp}"; then
        echo "Installing missing Rust component $cmp for $RUST_VERSION..."
        step rustup component add --toolchain "$RUST_VERSION" "$cmp"
    fi
done

# The sub-scripts cross-compile (cargo-zigbuild) and check for these targets; install any the
# pinned toolchain is missing, else the build fails with "can't find crate for core".
INSTALLED_TARGETS="$(rustup target list --installed)"
if [ $? -ne 0 ]; then
    fail "could not list the installed Rust targets for $RUST_VERSION"
fi
for tgt in aarch64-apple-darwin x86_64-unknown-linux-gnu x86_64-pc-windows-gnu wasm32-unknown-unknown; do
    if ! echo "$INSTALLED_TARGETS" | grep -qx "$tgt"; then
        echo "Installing missing Rust target $tgt for $RUST_VERSION..."
        step rustup target add "$tgt"
    fi
done

# docs.rs and the public-api check use the latest rolling nightly; require it up to date so
# nightly-only lint/rustdoc drift is caught here rather than in CI.
NIGHTLY_CHECK="$(rustup check 2>/dev/null | grep '^nightly-')"
if [ -z "$NIGHTLY_CHECK" ]; then
    echo "Rust nightly is not installed (needed for docs and the public-api check). Install it with:"
    echo "  rustup toolchain install nightly --profile minimal"
    exit 1
fi
if echo "$NIGHTLY_CHECK" | grep -qi "update available"; then
    echo "Rust nightly is out of date; CI and docs.rs use the latest nightly. Update it with:"
    echo "  rustup update nightly"
    exit 1
fi

# Every formatter gate, before anything expensive so a reformat is a one-line fix. The same
# per-language scripts CI's format, format_python, format_typescript and test_dart_format jobs run.
echo "Performing format check..."
step ./scripts/check_format.sh

# Crate READMEs are generated from the crate docs; same script CI's test_readme_sync job runs.
echo "Performing README sync check..."
step ./scripts/check_readme_sync.sh

# CI assembles its container from an explicit Earthfile allowlist, so a root-level config
# that is not listed there is absent in CI while every local run still finds it.
echo "Checking Earthfile source coverage..."
if ! python3 - <<'PY'
import re, subprocess, sys

tracked = subprocess.run(
    ["git", "ls-files", "--full-name", "."], capture_output=True, text=True, check=True
).stdout.split()
# Documentation and the CI/release definitions themselves are not build inputs
skip = re.compile(r"\.md$|^LICENSE$|^\.|^Earthfile$|^dagger\.json$|^version_bump\.sh$")
want = {f for f in tracked if "/" not in f and not skip.search(f)}

targets = ("code-linux", "code-flutter", "code-flutter-rust", "code-android")
copied, current = {}, None
for line in open("Earthfile", encoding="utf-8"):
    m = re.match(r"^([A-Za-z0-9_.-]+):\s*$", line)
    if m:
        current = m.group(1)
    elif current in targets and "COPY" in line and "Cargo.toml" in line:
        copied.setdefault(current, set()).update(line.split())

rc = 0
for t in targets:
    missing = sorted(want - copied.get(t, set()))
    if missing:
        rc = 1
        print(f"Earthfile target {t} does not copy: {' '.join(missing)}", file=sys.stderr)
sys.exit(rc)
PY
then
    fail "the CI container is missing a root-level build config. Add the files above to that target's COPY line in the Earthfile, or exclude them in check_all.sh if CI does not need them."
fi

# The Earthfile's base FROM runs before .dagger/versions.env has been copied in, so it cannot read
# the value and duplicates the literal instead. Dagger reads versions.env directly, so a drift here
# silently builds the two on different images.
echo "Checking build image agreement..."
earthfile_image=$(sed -n 's/^ARG --global LINUX_BUILD_IMAGE=\(.*\)$/\1/p' Earthfile)
versions_image=$(. .dagger/versions.env && echo "$LINUX_BUILD_IMAGE")
if [ "$earthfile_image" != "$versions_image" ]; then
    fail "Earthfile LINUX_BUILD_IMAGE ($earthfile_image) does not match .dagger/versions.env ($versions_image)."
fi

# CI's format job (rustfmt) runs on the upstream rust image rather than the veilid toolchain (it is
# a cheap standalone job, not an earthly one), so its version is a literal that can drift from the
# pin. rustfmt's output changes between releases, so a drift means CI and this script disagree.
ci_rust=$(sed -n 's/^  image: rust:\(.*\)-slim$/\1/p' .gitlab-ci.yml)
versions_rust=$(. .dagger/versions.env && echo "$RUST_VERSION")
if [ "$ci_rust" != "$versions_rust" ]; then
    fail "format job image (rust:$ci_rust-slim) does not match RUST_VERSION ($versions_rust) in .dagger/versions.env."
fi

# Source-only and sub-second, so it runs with the format check rather than with the
# compiling steps. Self-test first, so a broken gate reports itself rather than passing.
echo "Performing census check..."
step ./scripts/test_census.sh
step env CENSUS_CHECK_ONLY=1 ./scripts/update_census.sh

# Source-only like the census; same script CI's lint_python job runs.
echo "Performing Python lint..."
step ./scripts/_lint_python.sh

echo "Running all tests..."

# cargo-public-api (nightly rustdoc) and cargo-msrv (per-crate rust-version) select their own
# toolchains; don't force the pin on them.
step env -u RUSTUP_TOOLCHAIN ./scripts/_check_public_api.sh
step env -u RUSTUP_TOOLCHAIN ./scripts/_msrv_all.sh
step ./scripts/_lint_all.sh $CLEANUP
step ./scripts/_smoketest_all.sh $CLEANUP --locked
# skip cleanup for WASM here because the clippy script does WASM first and cleans up there
step ./scripts/_check_wasm_builds.sh dart
step ./scripts/_check_wasm_builds.sh js
# eslint's type-aware rules read the wasm package the step above built
step ./scripts/_lint_typescript.sh
step ./scripts/_build_docs.sh $CLEANUP
step ./scripts/_unit_tests_all.sh
step env DEFAULT_CARGO_TARGET=wasm32-unknown-unknown ./scripts/_unit_tests_all.sh
# Desktop flutter binding, exercised end to end against the in-process mock. Headless, so
# it is the same run here as in CI - no window, everything on stdout.
step ./scripts/_flutter_integration_tests.sh

echo "check_all: all checks passed."
