# Veilid Dagger Migration

This document details the migration from Earthly to Dagger for the Veilid project's build system.

## Quick Start

Recommended build configuration is:

* AMD64 or ARM64 machine
* Linux or MacOS
* At least 8GB of RAM
* At least 100GB of free disk space
* At least 4 CPU cores

First, ensure a container runtime is installed. Tested runtimes include:

* Docker
    - If you are using Docker, things should work out of the box with the above allocations on the host.
* Podman
    - If you are using Podman, you will need to configure it. See [instructions at the end of this document](#using-podman).

After your container runtime is avalable, [install dagger](https://docs.dagger.io/install/),

### Common Commands

All command execute from the root of the `veilid` repository:

```bash
# Install dependencies and run tests
dagger -c 'test-all $(host | directory --gitignore .)'

# Build for specific architectures
dagger -c 'build-linux-amd-64 $(host | directory --gitignore .)'
dagger -c 'build-linux-arm-64 $(host | directory --gitignore .)'

# Package for distribution
dagger -c 'package-linux $(host | directory --gitignore .) | export ./target/dist'

# Run individual gates
dagger -v 'lint $(host | directory --gitignore .)'
dagger -v 'census $(host | directory --gitignore .)'
dagger -v 'test-unit-tests-all $(host | directory --gitignore .)'
```

Pass the source with `$(host | directory --gitignore .)` rather than `--source .`. Dagger cannot
apply `.gitignore` to a function argument, so `--source .` falls back to the short artifact-tree
list in `.dagger/src/veilid/ignore_patterns.py`; the `--gitignore` form filters from the repo's
own `.gitignore` files and stays correct as they change.

## Function Reference

### Dependencies and Base Images

| Dagger Function | Earthly Target      | Description                                                                |
| --------------- | ------------------- | -------------------------------------------------------------------------- |
| `deps_base()`   | `deps-base`         | Install build prerequisites (Ubuntu 18.04, CMake, cross-compilation tools) |
| `tool_rust()`   | `tool-rust`         | Rust toolchain: pinned compiler, clippy, every target in versions.env, test nightly |
| `tool_cargo_bins()` | `tool-cargo-bins` | cargo-binstall, msrv, nextest, docs-rs, public-api, sync-readme             |
| `tool_astgrep()` | `tool-astgrep`     | ast-grep, the census gate engine (source build)                            |
| `tool_zigbuild()` | `tool-zigbuild`   | cargo-zigbuild (source build)                                              |
| `tool_zig()`    | `tool-zig`          | Zig, for cross-compilation                                                 |
| `tool_binaryen()` | `tool-binaryen`   | wasm-opt                                                                   |
| (n/a)           | `tool-flutter`      | Flutter SDK; in Dagger the SDK install lives in `deps_flutter()`           |
| `deps_rust()`   | `deps-rust`         | Every cached tool assembled into one toolchain                             |
| `deps_linux()`  | `deps-linux`        | Linux build dependencies (equivalent to deps-rust in Dagger)               |
| `fetch_cache()` | `fetch-cache`       | Pre-download every crate in Cargo.lock so no job depends on reaching crates.io |
| `deps_flutter()` | `deps-flutter`     | Flutter SDK on `FLUTTER_BUILD_IMAGE`, no Rust (amd64-only)                 |
| `code_flutter_rust()` | `code-flutter-rust` | Source plus the Rust toolchain, for the gates that build the native library |
| `publish_crate_cache()` | `fetch-cache` + `SAVE IMAGE --push` | Publish the crate download cache to the registry as `crate-cache:<crate-cache-tag>` |

Each tool is cached on its own version. Earthly keeps `+tool-<name>` as a registry image
tagged by `scripts/tool_tags.sh`, so a bump to one version in `.dagger/versions.env` rebuilds
that tool and nothing else; CI passes `--TOOLS=container` to pull them and `build_tools`
pushes any tag that is missing. Dagger needs no registry — one function per tool is enough for
the engine to cache each on its own inputs. The `registry_cleanup` job (scheduled, or run by
hand from the web UI) keeps the 3 most recent tags per `tool-*` repository and in `crate-cache`,
plus whichever tag the build currently asks for.

### Source and Code Preparation

| Dagger Function | Earthly Target | Description                                                              |
| --------------- | -------------- | ------------------------------------------------------------------------ |
| `code_linux()`   | `code-linux`   | Import source code with build dependencies, supports multiple base modes |
| `code_flutter()` | `code-flutter` | Import source code on top of the Flutter SDK                             |

**Crate cache:** `--crate-cache-tag` (Earthly: `--CRATE_CACHE_TAG`) names a published
`crate-cache:<tag>` image to start from. Unset — the default — downloads crates during the
build. Nothing compiled is cached either way; the cache exists so that no job depends on
reaching crates.io, not to make builds warm.

The cache lives in `CARGO_HOME`, outside `/veilid`, which is what lets both builders share one
code path. Dagger's `with_directory` replaces its mount path where Earthly's `COPY` merges, so
anything cached *under* `/veilid` would need capturing before the source mount and re-injecting
after in one builder and not the other. Keeping it in `CARGO_HOME` removes that asymmetry, and
with it the class of bugs where a mis-ordered call silently wipes the cache.

**CI tag convention:** `scripts/cache_tags.sh crate` prints the tag — a digest of `Cargo.lock`,
which is exactly and only what decides the download set. `fetch_cache` publishes on a miss, so a
dependency bump builds one new tag and every pipeline on the same lockfile shares one image.
Consumers fall back to downloading when the tag is absent, so a missing image is a slow build,
never a failed one.

### Linting and Code Quality

| Dagger Function       | Earthly Target        | Description                                                  |
| --------------------- | --------------------- | ------------------------------------------------------------ |
| `lint()`              | `lint`                | Run every clippy target in one pass (`scripts/_lint_all.sh`)  |
| `test_clippy_*()`     | `test-clippy-*`       | One function per clippy target, for parallel CI jobs          |
| `census()`            | `census`              | Census rule self-test, then the inventory drift check         |
| `dart_format()`       | `test-dart-format`    | Dart format gate over veilid-flutter                          |
| `public_api()`        | `public-api`          | Public API drift check                                        |
| `msrv()`              | `msrv`                | MSRV check for every crate                                    |
| `smoketest()`         | `smoketest`           | Smoketest for every published crate                           |

The `aarch64-apple-darwin` clippy target runs; no Apple binary is built (see
[Cross-Compilation Status](#cross-compilation-status)).

### Building

| Dagger Function         | Earthly Target        | Description                               |
| ----------------------- | --------------------- | ----------------------------------------- |
| `build_linux_amd64()`   | `build-linux-amd64`   | Build release binaries for x86_64 Linux   |
| `build_linux_arm64()`   | `build-linux-arm64`   | Build release binaries for aarch64 Linux  |
| `build_windows_amd64()` | `build-windows-amd64` | Build release binaries for x86_64 Windows |

**Returns:** `dagger.Directory` containing built artifacts (not local files like Earthly's `SAVE ARTIFACT`)

### Testing

| Dagger Function              | Earthly Target(s)          | Description                                              |
| ---------------------------- | -------------------------- | -------------------------------------------------------- |
| `test_docs()`                | `package-tests-docs-linux` | Build docs against a given nightly                        |
| `test_unit_tests_all()`      | `test-unit-tests-all`      | Run the Rust unit tests                                   |
| `test_census()`              | `test-census`              | Census gates                                              |
| `test_dart_format()`         | `test-dart-format`         | Dart format gate                                          |
| `test_flutter_integration()` | `test-flutter-integration` | veilid-flutter against the in-process mock, headless      |
| `test_all()`                 | `test-all`                 | Every gate, serially — for a desktop, not for CI          |

`test_all()` runs the gates in one process, one after another. CI fans the individual
`test_*` functions out instead.

### Packaging

| Dagger Function           | Earthly Target          | Description                                          |
| ------------------------- | ----------------------- | ---------------------------------------------------- |
| `package_deb()`           | `package-linux-*-deb`   | Create DEB packages for specified Linux architecture |
| `package_rpm()`           | `package-linux-*-rpm`   | Create RPM packages for specified Linux architecture |
| `package_linux_amd64()`   | `package-linux-amd64`   | Create both DEB and RPM packages for x86_64 Linux    |
| `package_linux_arm64()`   | `package-linux-arm64`   | Create both DEB and RPM packages for aarch64 Linux   |
| `package_linux()`         | `package-linux`         | Create packages for all Linux architectures          |
| `package_windows_amd64()` | `package-windows-amd64` | Create ZIP packages for Windows x86_64               |
| `package_windows()`       | `package-windows`       | Create packages for all Windows architectures        |

## Key Differences: Earthly vs Dagger

### Architecture & Patterns

| Aspect                | Earthly                      | Dagger                                         |
| --------------------- | ---------------------------- | ---------------------------------------------- |
| **File artifacts**    | `SAVE ARTIFACT ... AS LOCAL` | Return `dagger.Directory`, use `export --path` |
| **Parallelization**   | `WAIT` + `BUILD` blocks      | Automatic via Dagger's execution engine        |
| **Caching**           | Manual registry push/pull    | Built-in content-addressed caching             |
| **Cross-compilation** | Multiple toolchain packages  | Simplified with Zig (zigbuild)                 |
| **Container reuse**   | `FROM +target` references    | Function composition and reuse                 |

### Specific Changes

#### 1. **Artifact Handling**
```bash
# Earthly
SAVE ARTIFACT ./target/x86_64-unknown-linux-gnu AS LOCAL ./target/artifacts/x86_64-unknown-linux-gnu

# Dagger  
dagger -c 'build-linux-amd-64 $(host | directory --gitignore .) | export ./target/artifacts/x86_64-unknown-linux-gnu'
```

#### 2. **Gate Granularity**
- Each Earthly `test-*` target has one `test_*` function, so CI can fan them out as
  separate jobs; `test_all` runs the same set serially for a desktop.

#### 3. **Cross-compilation Simplification**
- **Earthly:** Complex MinGW, cross-gcc, libc-dev packages
- **Dagger:** Primarily uses `cargo zigbuild` for cross-compilation

#### 4. **Output Visibility**
- **Earthly:** Build output visible by default
- **Dagger:** Test functions return `str` with captured output for visibility

## Functions Not Migrated

### Skipped Functions
| Earthly Target      | Reason                                       |
| ------------------- | -------------------------------------------- |
| `deps-android`      | No Android build in CI yet                   |
| `code-android`      | No Android build in CI yet                   |
| `build-android`     | No Android build in CI yet                   |
| `build-macos-arm64` | Commented out in the Earthfile               |

### Functions That Didn't Need Migration
- **Wait/Build orchestration:** Handled automatically by Dagger's execution model

## Cross-Compilation Status

| Target                      | Status     | Notes                                      |
| --------------------------- | ---------- | ------------------------------------------ |
| `x86_64-unknown-linux-gnu`  | ✅ Working  | Uses cross-compilation toolchain           |
| `x86_64-pc-windows-gnu`     | ✅ Working  | Uses MinGW-w64                             |
| `aarch64-unknown-linux-gnu` | ✅ Working  | Uses cross-compilation toolchain           |
| `aarch64-apple-darwin`      | ⚠️ Lint only | Clippy runs; linking needs osxcross         |
| `wasm32-unknown-unknown`    | ✅ Working  | Native WASM support                        |

## Host Architecture

`deps_base` through `package_*` run on either amd64 or arm64. The Flutter gates
(`dart_format`, `flutter_integration`) do not: Flutter ships a linux-x64 SDK only, so
`deps_flutter` fails fast on an arm64 engine rather than half-installing.

## versions.env

Every pinned version comes from `.dagger/versions.env`, which the Earthfile, the
`dev-setup` scripts and this module all read. The module imports it by path at load time
(the module runtime mounts `.dagger`, not the repo root).

`DIRECTORY_IGNORE_PATTERNS` drops `.dagger` from the source upload, so the file is not in
the mounted tree. `code_flutter` writes it back to `/veilid/.dagger/versions.env`, which
`scripts/_dart_sources.sh` and `scripts/_flutter_integration_tests.sh` both source. A gate
that needs a pin from versions.env and runs on `code_linux` has to do the same.

## Usage Examples

### Development Workflow
```bash
# Quick development check
dagger -c 'lint $(host | directory --gitignore .)'

# Run all tests
dagger -c 'test-all $(host | directory --gitignore .)'

# Build for production
dagger -c 'build-linux-amd-64 $(host | directory --gitignore .) | export ./target/artifacts/x86_64-unknown-linux-gnu'
```

### CI/CD Pipeline
```bash
# Full pipeline: test, build, and package
dagger -c 'test-all $(host | directory --gitignore .)'
dagger -c 'package-linux $(host | directory --gitignore .) | export ./target/dist'

# Nightly builds
dagger -c 'package-linux $(host | directory --gitignore .) --is-nightly true | export ./target/dist-nightly'
```

### Cache Management
```bash
# Use uncached mode for clean builds
dagger -c 'test-all $(host | directory --gitignore .)'

# Use container mode for CI (with remote cache)
dagger -c 'test-all $(host | directory --gitignore .) --crate-cache-tag 3289c6b3 --ci-registry-image registry.example.com/veilid/veilid'
```

### Using Podman
Podman requires configuring the podman machine used to build Veilid.

```bash
podman machine init -m 10240 --now podman-machine-default
podman machine ssh podman-machine-default sudo modprobe iptable_nat
podman machine ssh podman-machine-default sudo setenforce Permissive
```

A script in `scripts/podman_setup.sh` exists to make this process easier as it may need to happen
whenever you start the podman machine. It is idempotent — run it any time to create the machine if
missing, start it if stopped, and (re)apply the configuration. Build failures can result if these
configurations are not met.

### Development on the Dagger build itself

```bash
# Install Dagger SDK
cd .dagger
dagger develop --sdk=python

# Create virtual environment
uv venv

# Install dependencies in virtual environment
uv sync
```

Now, point your development environment to the `.dagger/.venv/bin/python` interpreter

## What the crate cache costs and buys

Measured on the committed lockfile (670 packages, 660 of them external):

| | |
| --- | --- |
| `cargo fetch --locked` | ~10s |
| `registry/cache` + `registry/index` (what ships) | 152 MB |
| `registry/src` (dropped; re-extracted locally, no network) | a further 895 MB |

It replaces a `cargo-chef` dependency prebuild that took ~40 minutes of serial cooking, ran
all-or-nothing, produced a multi-gigabyte image, and matched only five of the nine consuming
configurations. What it buys is narrower and unconditional: no job depends on reaching
crates.io. What it gives up is warm dependency artifacts — every job now compiles its own
dependencies, in parallel, which is roughly what a cold pipeline already cost.

`fetch_cache` asserts completeness by re-running the fetch with `--offline` after dropping
`registry/src`. An image that exists but is short a crate is otherwise a silent downstream
failure: every job quietly reaches the network again.

## Migration Notes

- Cache control is `--crate-cache-tag`; there is no `base` parameter
- Package functions now organize output in structured directories
- Test functions provide detailed output for better CI integration
- Cross-compilation is more reliable with Zig toolchain
- Memory checks removed (Dagger handles resource management)