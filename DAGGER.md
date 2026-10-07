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

# Run individual operations
dagger -v 'clippy $(host | directory --gitignore .)'
dagger -v 'test-native $(host | directory --gitignore .)'
```

## Function Reference

### Dependencies and Base Images

| Dagger Function | Earthly Target      | Description                                                                |
| --------------- | ------------------- | -------------------------------------------------------------------------- |
| `deps_base()`   | `deps-base`         | Install build prerequisites (Ubuntu 18.04, CMake, cross-compilation tools) |
| `deps_rust()`   | `deps-rust`         | Install Rust toolchain, targets, cargo tools, and Zig                      |
| `deps_linux()`  | `deps-linux`        | Linux build dependencies (equivalent to deps-rust in Dagger)               |
| `deps_cache()`  | `build-linux-cache` | Pre-compile Rust dependencies using cargo-chef for faster builds           |
| `publish_debug_cache()` | `build-linux-cache` + `SAVE IMAGE --push` | Publish the cooked dependency cache to the registry as `build-cache:<cache-tag>` (cook layered at `/baked/target`) |

### Source and Code Preparation

| Dagger Function | Earthly Target | Description                                                              |
| --------------- | -------------- | ------------------------------------------------------------------------ |
| `code_linux()`  | `code-linux`   | Import source code with build dependencies, supports multiple base modes |

**Base modes:**
- `local` (default): Uses local deps_cache 
- `uncached`: Uses deps_linux without cache
- `container`: Uses the remote registry cache image `build-cache:<cache-tag>` (`--cache-tag`, default `latest`)

Dagger's `with_directory` replaces the mount path (unlike Earthly `COPY`, which merges),
so `code_linux` captures the cooked cargo target before mounting source and re-injects
it at `/veilid/target` afterwards — the same absolute path the cook used, keeping the
chef fingerprints valid.

**Expected partial cache misses (by design, not regressions):** the cooks build 5
packages, while the clippy scripts check the whole workspace — resolver-v2 feature
unification can give some dependencies different feature sets than the cook, and
`_unit_tests_all.sh` adds `--features=debug-locks` the cook doesn't; those deps
recompile once per run.

**CI tag convention (for the Earthly→Dagger switch):** the module is tag-agnostic;
CI computes content-keyed tags like `debug-<branch>-<toolkey>-<rustkey>-<arch>`
(toolkey = versions.env + module source hash; rustkey = Cargo.lock + manifests hash;
images are single-arch, so the consumer arch must be in the tag). See veilidchat's
`.gitlab-ci.yml` for the working pattern.

### Linting and Code Quality

| Dagger Function | Earthly Target | Description                                             |
| --------------- | -------------- | ------------------------------------------------------- |
| `clippy()`      | `clippy`       | Run clippy linting for Linux, Windows, and WASM targets |

**Note:** macOS target is commented out due to cross-compilation complexity.

### Building

| Dagger Function         | Earthly Target        | Description                               |
| ----------------------- | --------------------- | ----------------------------------------- |
| `build_linux_amd64()`   | `build-linux-amd64`   | Build release binaries for x86_64 Linux   |
| `build_linux_arm64()`   | `build-linux-arm64`   | Build release binaries for aarch64 Linux  |
| `build_windows_amd64()` | `build-windows-amd64` | Build release binaries for x86_64 Windows |

**Returns:** `dagger.Directory` containing built artifacts (not local files like Earthly's `SAVE ARTIFACT`)

### Testing

| Dagger Function | Earthly Target(s)         | Description                                     |
| --------------- | ------------------------- | ----------------------------------------------- |
| `test_docs()`   | `unit-tests-docs-linux`   | Build and test documentation                    |
| `test_all()`    | `unit-tests-linux`        | Run all test suites: clippy, native, docs, WASM |

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

#### 2. **Function Consolidation**
- **Earthly:** 7 separate `unit-tests-*` targets
- **Dagger:** 2 focused test functions (`test_docs`, `test_all`)

#### 3. **Cross-compilation Simplification**
- **Earthly:** Complex MinGW, cross-gcc, libc-dev packages
- **Dagger:** Primarily uses `cargo zigbuild` for cross-compilation

#### 4. **Output Visibility**
- **Earthly:** Build output visible by default
- **Dagger:** Test functions return `str` with captured output for visibility

## Functions Not Migrated

### Skipped Functions
| Earthly Target                  | Reason                              |
| ------------------------------- | ----------------------------------- |
| `deps-android`                  | Skipped for initial migration       |
| `code-android`                  | Skipped for initial migration       |
| `build-android`                 | Skipped for initial migration       |
| `build-macos-arm64`             | Commented out in original Earthfile |
| `unit-tests-clippy-macos-linux` | needs macOS cross-compilation       |

### Functions That Didn't Need Migration
- **Individual clippy targets:** Consolidated into single `clippy()` function
- **Wait/Build orchestration:** Handled automatically by Dagger's execution model

## Cross-Compilation Status

| Target                      | Status     | Notes                                      |
| --------------------------- | ---------- | ------------------------------------------ |
| `x86_64-unknown-linux-gnu`  | ✅ Working  | Uses cross-compilation toolchain           |
| `x86_64-pc-windows-gnu`     | ✅ Working  | Uses MinGW-w64                             |
| `aarch64-unknown-linux-gnu` | ✅ Working  | Uses cross-compilation toolchain           |
| `aarch64-apple-darwin`      | ❌ Disabled | Requires osxcross or similar complex setup |
| `wasm32-unknown-unknown`    | ✅ Working  | Native WASM support                        |

## Usage Examples

### Development Workflow
```bash
# Quick development check
dagger -c 'clippy $(host | directory --gitignore .)'

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
dagger call test-all --source . --base uncached

# Use container mode for CI (with remote cache)
dagger call test-all --source . --base container --ci-registry-image registry.example.com/veilid/veilid
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

## Performance Benefits

1. **Better Caching:** Dagger's content-addressed caching is more efficient than Earthly's layer caching
2. **Parallel Execution:** Automatic parallelization without manual `WAIT`/`BUILD` blocks
3. **Simplified Cross-compilation:** Zig handles most cross-compilation complexity
4. **Artifact Management:** No need for manual registry management or local artifact copying

## Migration Notes

- All functions support the same `base` parameter for cache control
- Package functions now organize output in structured directories
- Test functions provide detailed output for better CI integration
- Cross-compilation is more reliable with Zig toolchain
- Memory checks removed (Dagger handles resource management)