#!/bin/sh
# Registry tag for the crate download cache.
#
# The Earthfile, the Dagger module and .gitlab-ci.yml all ask this script instead of spelling
# out a tag of their own, the same way they ask tool_tags.sh for the tool image tags.
#
# Two inputs. Cargo.lock decides which crates get downloaded -- not the manifests, not
# .cargo/config.toml. But the published image is FROM deps-linux, so it also carries the whole
# toolchain, and +code-linux starts from it: key on Cargo.lock alone and a rust bump would leave
# every test job running the OLD toolchain out of a crate-cache whose tag never moved. A
# collision would at worst mean fetching a few crates over the network, so a short digest is plenty.
#
# usage: cache_tags.sh crate    print the crate cache tag
set -eu

REPO_ROOT="${REPO_ROOT:-$(dirname "$0")/..}"
TOOL_TAGS="${TOOL_TAGS:-$(dirname "$0")/tool_tags.sh}"

# Alpine and ubuntu have sha256sum; macOS only shasum.
key() {
    { sha256sum 2>/dev/null || shasum -a 256; } | cut -c1-8
}

case "${1:?usage: cache_tags.sh crate}" in
    crate) { cat "$REPO_ROOT/Cargo.lock"; sh "$TOOL_TAGS" deps-linux; } | key ;;
    *)     echo "cache_tags: unknown cache '$1' (have: crate)" >&2; exit 1 ;;
esac
