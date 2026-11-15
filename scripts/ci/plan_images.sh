#!/bin/sh
# Decide which build images are already in the registry, for the image-build child pipeline.
#
# Runs in a curl-only container: no docker, no dind, no earthly. The registry v2 API answers
# 200/404 for a tag given nothing but a JWT, which is the whole point — a warm pipeline
# disqualifies every image in seconds instead of paying a ~110s earthly job to find nothing to do.
#
# Writes BUILD_<NAME>=true lines for the missing ones; the child pipeline's jobs are gated on
# those, so an image that is present simply has no job.
set -eu

OUT="${1:-images.env}"
HOST="${CI_SERVER_HOST:-gitlab.com}"
REGISTRY="${CI_REGISTRY:-registry.gitlab.com}"
PROJECT="${CI_PROJECT_PATH:?CI_PROJECT_PATH is required}"
: > "$OUT"

have() {  # have <image> <tag>
    tok=$(curl -sf --retry 3 --retry-connrefused \
        ${CI_JOB_TOKEN:+--user "gitlab-ci-token:$CI_JOB_TOKEN"} \
        "https://$HOST/jwt/auth?service=container_registry&scope=repository:$PROJECT/$1:pull" \
        | sed -n 's/.*"token":"\([^"]*\)".*/\1/p') || return 1
    [ -n "$tok" ] || return 1
    code=$(curl -s -o /dev/null -w '%{http_code}' -H "Authorization: Bearer $tok" \
        -H "Accept: application/vnd.docker.distribution.manifest.v2+json,application/vnd.docker.distribution.manifest.list.v2+json,application/vnd.oci.image.index.v1+json" \
        "https://$REGISTRY/v2/$PROJECT/$1/manifests/$2")
    [ "$code" = "200" ]
}

emit() {  # emit <VAR> <image> <tag>
    if have "$2" "$3"; then
        echo "cached   $2:$3"
    else
        echo "MISSING  $2:$3"
        echo "$1=true" >> "$OUT"
    fi
}

t() { sh scripts/tool_tags.sh "$1"; }

emit BUILD_DEPS_BASE       deps-base       "$(t deps-base)"
emit BUILD_TOOL_FLUTTER    tool-flutter    "$(t flutter)"
emit BUILD_TOOL_RUST       tool-rust       "$(t rust)"
emit BUILD_TOOL_ZIG        tool-zig        "$(t zig)"
emit BUILD_TOOL_BINARYEN   tool-binaryen   "$(t binaryen)"
emit BUILD_TOOL_CARGO_BINS tool-cargo-bins "$(t cargo-bins)"
emit BUILD_TOOL_ASTGREP    tool-astgrep    "$(t astgrep)"
emit BUILD_TOOL_ZIGBUILD   tool-zigbuild   "$(t zigbuild)"
emit BUILD_DEPS_LINUX      deps-linux      "$(t deps-linux)"
emit BUILD_DEPS_FLUTTER    deps-flutter    "$(t deps-flutter)"
emit BUILD_DEPS_FLUTTER_RUST deps-flutter-rust "$(t deps-flutter-rust)"
emit BUILD_DEPS_XWIN       deps-xwin       "$(t deps-xwin)"
emit BUILD_CRATE_CACHE     crate-cache     "$(sh scripts/cache_tags.sh crate)"

echo "--- $OUT ---"
cat "$OUT"
