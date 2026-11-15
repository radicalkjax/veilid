#!/bin/sh
# Publish the prebuilt veilid-flutter library +prebuilt-flutter-windows-x64 saved, then prune.
#
# One package per target and one version per commit, <crate version>-<sha12>, which is how a
# Windows app build finds the library for the veilid commit it compiles against (agent-memory
# veilid/veilid-flutter-prebuilt.md). A published version is never replaced, so a hash a consumer
# pinned stays valid, and the build of a v* tag's commit is never pruned.
#
# usage: publish_prebuilt.sh <dir holding veilid_flutter.dll, veilid_flutter.dll.sha256, manifest.json>
set -eu

DIR="${1:?usage: publish_prebuilt.sh <dir>}"
PACKAGE=veilid-flutter-x86_64-pc-windows-msvc
KEEP=7
API="$CI_API_V4_URL/projects/$CI_PROJECT_ID"
VERSION=$(jq -r .version "$DIR/manifest.json")
URL="$API/packages/generic/$PACKAGE/$VERSION"

# The .sha256 goes up after the library, so its presence means the version is complete
if curl -sf -o /dev/null --header "JOB-TOKEN: $CI_JOB_TOKEN" "$URL/veilid_flutter.dll.sha256"; then
    echo "$PACKAGE $VERSION is already published; leaving it as it is"
else
    for file in veilid_flutter.dll veilid_flutter.dll.sha256 manifest.json; do
        curl -sSf --header "JOB-TOKEN: $CI_JOB_TOKEN" --upload-file "$DIR/$file" "$URL/$file"
        echo
    done
    echo "published $PACKAGE $VERSION"
fi

# A CI clone carries no tags, so the tagged commits come from the API. Without them nothing is
# pruned, rather than risking a release's build.
tagged=$(mktemp)
versions=$(mktemp)
page=1
while :; do
    resp=$(curl -sf "$API/repository/tags?per_page=100&page=$page") || {
        echo "could not list tags; not pruning"
        exit 0
    }
    [ "$(printf '%s' "$resp" | jq 'length')" -gt 0 ] || break
    printf '%s\n' "$resp" | jq -r '.[] | select(.name | test("^v[0-9]")) | .commit.id[0:12]' >> "$tagged"
    page=$((page + 1))
done
page=1
while :; do
    resp=$(curl -sf --header "JOB-TOKEN: $CI_JOB_TOKEN" \
        "$API/packages?package_type=generic&package_name=$PACKAGE&per_page=100&page=$page") || break
    [ "$(printf '%s' "$resp" | jq 'length')" -gt 0 ] || break
    printf '%s\n' "$resp" | jq -c --arg name "$PACKAGE" '.[] | select(.name == $name)' >> "$versions"
    page=$((page + 1))
done
jq -rs --rawfile tagged "$tagged" --argjson keep "$KEEP" '
    ($tagged | split("\n") | map(select(length > 0))) as $tags
    | map(select(.version[-12:] as $sha | $tags | any(. == $sha) | not))
    | sort_by(.created_at) | reverse | .[$keep:] | .[] | "\(.id) \(.version)"' "$versions" |
while read -r id version; do
    echo "pruning $PACKAGE $version"
    curl -sSf --request DELETE --header "JOB-TOKEN: $CI_JOB_TOKEN" "$API/packages/$id" \
        || echo "  delete failed (grant the job token ADMIN_PACKAGES: Settings > CI/CD > Job token permissions)"
done
rm -f "$tagged" "$versions"
