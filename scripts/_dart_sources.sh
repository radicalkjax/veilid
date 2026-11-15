# Shared Dart source walk and formatter settings. Source it from the repo root.
#
# One walk so scripts/fmt.sh and scripts/_check_format_dart.sh can never disagree about what is
# checked. Generated code is skipped for the same reason the analyzer skips it: nobody edits
# it, and a regenerator's output is not the author's fault.
#
# find + a filter rather than `dart format <dir>`, which has no exclude flag and would
# descend into per-package build output.
#
# veilidchat carries its own copy of this file rather than sourcing this one: it has to build
# against a pub.dev veilid-flutter, with no veilid checkout beside it. Keep the two in step.

DART_SOURCE_ROOTS=(veilid-flutter)

DART_GENERATED_SUFFIXES='\.(freezed|g)\.dart$'

# Format at the PINNED toolchain's language version, from DART_VERSION rather than from
# whatever dart is on PATH: dev-setup never downgrades flutter, so a developer can be on a
# newer SDK than CI. Without the flag `dart format` takes the version from a package's
# .dart_tool/package_config.json and falls back to the SDK's newest where a package is
# unresolved, so the same file formats one way locally and another in CI.
source .dagger/versions.env
if [ -z "${DART_VERSION:-}" ]; then
    echo "format: DART_VERSION is not set in .dagger/versions.env" >&2
    exit 1
fi
DART_FORMAT_LANGUAGE_VERSION="${DART_VERSION%.*}"

dart_source_files() {
    find "${DART_SOURCE_ROOTS[@]}" \
        \( -name build -o -name .dart_tool -o -name .symlinks -o -name ephemeral \) -prune \
        -o -name '*.dart' -print \
        | grep -vE "$DART_GENERATED_SUFFIXES" \
        | sort
}
