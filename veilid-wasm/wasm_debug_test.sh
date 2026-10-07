#!/bin/bash
SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
pushd "$SCRIPTDIR" &> /dev/null

OUTDIR="$HOME/testout"
TEST_NAME=""
SPEC_FILE=""

# Parse optional arguments
while [ $# -gt 0 ]; do
    arg="$1"
    case "$arg" in
        --out=*)
            OUTDIR="${arg#--out=}"
            echo "Setting OUTDIR to $OUTDIR"
            ;;
        --*)
            echo "Unknown argument: $arg"
            exit 1
            ;;
        *)
            break
            ;;
    esac
    shift
done
    
# Parse test name and spec file arguments
if [ $# -ge 1 ]; then
    if [[ "$1" == "empty-transaction-commit" ]]; then
        TEST_NAME="should create empty transaction and commit"
        SPEC_FILE="./src/VeilidRoutingContext.test.ts"
    elif [[ "$1" == "create-set-get-commit" ]]; then
        TEST_NAME="should create transaction, add sets, gets, and commit"
        SPEC_FILE="./src/VeilidRoutingContext.test.ts"
    elif [[ "$1" == "create-fail-set-rollback" ]]; then
        TEST_NAME="should create empty transaction, fail non-transactional sets and then rollback"
        SPEC_FILE="./src/VeilidRoutingContext.test.ts"
    elif [[ "$1" == "create-set-commit-create-get-commit" ]]; then
        TEST_NAME="should create transaction, inspect, add sets to subkey 1, inspect, and commit. Then a new transaction inspect, and gets, and commit"
        SPEC_FILE="./src/VeilidRoutingContext.test.ts"
    elif [ $# -ge 2 ]; then
        TEST_NAME="$1"
        SPEC_FILE="$2"
    else
        TEST_NAME="$1"
    fi
else
    echo "Usage: $0 [--out=OUTDIR] [test_name] [spec_file]"
    echo "test_name can be a short name or a grep pattern as well"
    echo "spec_file is the relative path to the spec file to run from the tests directory"
fi

# Check if OUTDIR is a valid directory
if [ ! -d "$OUTDIR" ]; then
    echo "Error: OUTDIR is not a valid directory: $OUTDIR"
    exit 1
fi

# Construct output file name
OUTFILE="$OUTDIR/wasm-debug-test-$(date +%Y%m%d-%H%M%S).log"

echo "Writing output to $OUTFILE"

# Build arguments for wasm_test_js.sh
if [ -n "$SPEC_FILE" ]; then
    echo "Running test from spec file: \"$SPEC_FILE\""
    SPEC_COMMAND="--spec"
else
    echo "No spec file specified, running all test specs"
    SPEC_COMMAND=""
fi
if [ -n "$TEST_NAME" ]; then
    echo "Running test with name: \"$TEST_NAME\""
    TEST_COMMAND="--mochaOpts.grep"
else
    echo "No test name specified, running all test names"
    TEST_COMMAND=""
fi

# Run wasm_test_js.sh and tee output to the output file
./wasm_test_js.sh --debug --verbose-tracing -- $SPEC_COMMAND "$SPEC_FILE" $TEST_COMMAND "$TEST_NAME" 2>&1 | tee "$OUTFILE"
status=${PIPESTATUS[0]}
echo "Output written to $OUTFILE"

popd &> /dev/null
exit $status