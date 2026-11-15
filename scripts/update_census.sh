#!/bin/bash
# Regenerate the census inventory, or check it, mirroring update_public_api.sh.
#
#   scripts/update_census.sh                     regenerate, showing the diff and asking
#   CENSUS_CHECK_ONLY=1 scripts/update_census.sh the gate: fail on drift, change nothing
#
# The inventory counts, per rule and file, the sites a census rule matches. A reformat
# changes no count. That is the point: anything keyed on line positions breaks the next
# time a formatter moves a comment.
#
# A rule listed in scripts/census/strict-rules.txt must stay at zero. Its violations are never
# accepted into the inventory; the fix is to declare the site, not to record it.
#
# Deliberately duplicated with veilidchat's scripts/update_census.sh: only the engine lines
# differ (ast-grep here, an analyzer-based Dart tool there). veilidchat must not depend on
# this checkout existing beside it - veilid-flutter is headed for pub.dev - so the copy is
# the price of that, and it is a small one. Change one, change the other.

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
cd "$SCRIPTDIR/.." || exit 2

CENSUS_FILE=scripts/census/census.txt
STRICT_RULES=scripts/census/strict-rules.txt
SGCONFIG=sgconfig.yml

if ! command -v ast-grep > /dev/null 2>&1; then
    echo "ast-grep is not installed. Please install it with: cargo install ast-grep --locked"
    exit 1
fi

CENSUS_TMP=$(mktemp) || exit 2
CENSUS_DIFF=$(mktemp) || exit 2
trap 'rm -f "$CENSUS_TMP" "$CENSUS_DIFF"' EXIT

echo "Generating census..."
"$SCRIPTDIR/_census.sh" generate "$SGCONFIG" "$CENSUS_TMP" || exit $?

# A strict rule with any recorded site is a failure the inventory must not absorb.
if [ -f "$STRICT_RULES" ]; then
    violations=""
    while read -r rule; do
        case "$rule" in ''|\#*) continue ;; esac
        hits=$(awk -v r="$rule" '$2 == r { n += $1 } END { print n+0 }' "$CENSUS_TMP")
        if [ "$hits" -ne 0 ]; then
            violations="$violations  $rule: $hits undeclared site(s)"$'\n'
        fi
    done < "$STRICT_RULES"
    if [ -n "$violations" ]; then
        echo "Census gate FAILED: strict rules must have no undeclared sites:" >&2
        printf '%s' "$violations" >&2
        echo "Run 'ast-grep scan' to list them. Declare each site; do not record it here." >&2
        exit 1
    fi
fi

echo "Diffing census..."
diff -u -U 0 "$CENSUS_FILE" "$CENSUS_TMP" > "$CENSUS_DIFF"
diff_status=$?

if [ $diff_status -eq 0 ]; then
    echo "Census is up to date."
    exit 0
elif [ $diff_status -ne 1 ]; then
    echo "Errors found diffing the census." >&2
    exit $diff_status
fi

echo "Census changes"
echo "------------------------------------"
cat "$CENSUS_DIFF"
echo ""

if [ "$CENSUS_CHECK_ONLY" == "1" ]; then
    echo "Census changes found! Review the diff and run 'scripts/update_census.sh' to accept it."
    echo "A line appearing means a new undeclared site; a line vanishing can mean the rule"
    echo "stopped matching, which is a broken gate rather than a clean tree."
    exit 1
fi

read -r -p "Do you want to accept the diffs? (y/n) " ACCEPT_DIFF
if [ "$ACCEPT_DIFF" != "y" ]; then
    echo "Diff not accepted. Exiting."
    exit 1
fi
mv "$CENSUS_TMP" "$CENSUS_FILE"
echo "Census updated."
