#!/bin/bash
SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
pushd $SCRIPTDIR/.. >/dev/null

# Ensure cargo-public-api is installed
if ! command -v cargo-public-api > /dev/null 2>&1; then
    echo "cargo-public-api is not installed. Please install it with: cargo install cargo-public-api --locked"
    exit 1
fi

# Ensure rust nightly is installed
if ! rustup toolchain list | grep "nightly" > /dev/null 2>&1; then
    echo "rust nightly is not installed. Please install it with: rustup install nightly --profile minimal"
    exit 1
fi

# Ensure rust nightly target for linux x86_64 is installed
if ! rustup target list --installed --toolchain nightly | grep "x86_64-unknown-linux-gnu" > /dev/null 2>&1; then
    echo "rust nightly target for Linux x86_64 is not installed. Please install it with: rustup target add --toolchain nightly x86_64-unknown-linux-gnu"
    exit 1
fi

# Define public API generation constants
PUBLIC_API_TXT_FILE=public-api.txt
PUBLIC_API_TARGET=x86_64-unknown-linux-gnu
PUBLIC_API_FILE_VEILID_CORE=veilid-core/$PUBLIC_API_TXT_FILE
PUBLIC_API_FILE_VEILID_TOOLS=veilid-tools/$PUBLIC_API_TXT_FILE
PUBLIC_API_FILE_VEILID_REMOTE_API=veilid-remote-api/$PUBLIC_API_TXT_FILE
PUBLIC_API_FEATURES_VEILID_CORE="geolocation,footgun-nodeid-target,footgun-config"

# Make temporary file for the public API output
PUBLIC_API_TMP_FILE_VEILID_CORE=$(mktemp) || exit 2
PUBLIC_API_TMP_FILE_VEILID_TOOLS=$(mktemp) || exit 2
PUBLIC_API_TMP_FILE_VEILID_REMOTE_API=$(mktemp) || exit 2
PUBLIC_API_TMP_FILE_VEILID_CORE_DIFF=$(mktemp) || exit 2
PUBLIC_API_TMP_FILE_VEILID_TOOLS_DIFF=$(mktemp) || exit 2
PUBLIC_API_TMP_FILE_VEILID_REMOTE_API_DIFF=$(mktemp) || exit 2

# Ensure cleanup happens on exit, interruption, or termination
trap "rm -f $PUBLIC_API_TMP_FILE_VEILID_CORE $PUBLIC_API_TMP_FILE_VEILID_CORE_DIFF $PUBLIC_API_TMP_FILE_VEILID_TOOLS $PUBLIC_API_TMP_FILE_VEILID_TOOLS_DIFF $PUBLIC_API_TMP_FILE_VEILID_REMOTE_API $PUBLIC_API_TMP_FILE_VEILID_REMOTE_API_DIFF" EXIT

# Run cargo public-api and save the output to the temporary file
echo "Checking public API for 'veilid-core'..."
cargo public-api -sss -p veilid-core --target $PUBLIC_API_TARGET --color never --features $PUBLIC_API_FEATURES_VEILID_CORE > $PUBLIC_API_TMP_FILE_VEILID_CORE
echo "Checking public API for 'veilid-tools'..."
cargo public-api -sss -p veilid-tools --target $PUBLIC_API_TARGET --color never > $PUBLIC_API_TMP_FILE_VEILID_TOOLS
echo "Checking public API for 'veilid-remote-api'..."
cargo public-api -sss -p veilid-remote-api --target $PUBLIC_API_TARGET --color never > $PUBLIC_API_TMP_FILE_VEILID_REMOTE_API

# Define diff and print function
diff_and_print() {
    echo "Diffing public API for '$1'..."
    diff -u -U 0 $2 $3 > $4
    diff_status=$?
    if [ $diff_status -eq 0 ]; then
        echo "No public API changes found for '$1'."
        return 0
    elif [ $diff_status -eq 1 ]; then
        echo "Public API changes for '$1'"
        echo "------------------------------------"
        cat $4
        echo ""   
        return 1
    else
        echo "Errors found diffing '$1'."
        exit $diff_status
    fi
}

# Diff the temporary file with the current public API
diff_and_print "veilid-core" $PUBLIC_API_FILE_VEILID_CORE $PUBLIC_API_TMP_FILE_VEILID_CORE $PUBLIC_API_TMP_FILE_VEILID_CORE_DIFF
veilid_core_diff_status=$?
diff_and_print "veilid-tools" $PUBLIC_API_FILE_VEILID_TOOLS $PUBLIC_API_TMP_FILE_VEILID_TOOLS $PUBLIC_API_TMP_FILE_VEILID_TOOLS_DIFF
veilid_tools_diff_status=$?
diff_and_print "veilid-remote-api" $PUBLIC_API_FILE_VEILID_REMOTE_API $PUBLIC_API_TMP_FILE_VEILID_REMOTE_API $PUBLIC_API_TMP_FILE_VEILID_REMOTE_API_DIFF
veilid_remote_api_diff_status=$?

if [[ "$veilid_core_diff_status" -ne 0 || "$veilid_tools_diff_status" -ne 0 || "$veilid_remote_api_diff_status" -ne 0 ]]; then
    all_diff_status=1
else
    all_diff_status=0
fi

# Ask for confirmation or accept the diffs from the command line
if [ $all_diff_status -eq 1 ]; then

    if [[ "$PUBLIC_API_CHECK_ONLY" == "1" ]]; then 
        echo "Public API changes found! Review the diffs and run 'scripts/update_public_api.sh' to update the public API."
        exit 1
    fi

    read -p "Do you want to accept the diffs? (y/n) " ACCEPT_DIFF
    if [ "$ACCEPT_DIFF" != "y" ]; then
        echo "Diff not accepted. Exiting."
        exit 1
    fi

    # Replace the current public API with the new one
    if [ $veilid_core_diff_status -eq 1 ]; then
        mv $PUBLIC_API_TMP_FILE_VEILID_CORE $PUBLIC_API_FILE_VEILID_CORE
        echo "Public API updated for 'veilid-core'."
    fi
    if [ $veilid_tools_diff_status -eq 1 ]; then
        mv $PUBLIC_API_TMP_FILE_VEILID_TOOLS $PUBLIC_API_FILE_VEILID_TOOLS
        echo "Public API updated for 'veilid-tools'."
    fi
    if [ $veilid_remote_api_diff_status -eq 1 ]; then
        mv $PUBLIC_API_TMP_FILE_VEILID_REMOTE_API $PUBLIC_API_FILE_VEILID_REMOTE_API
        echo "Public API updated for 'veilid-remote-api'."
    fi
fi

popd >/dev/null
