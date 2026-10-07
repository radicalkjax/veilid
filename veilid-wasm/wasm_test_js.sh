#!/bin/bash
# pipefail so `cmd | tee` returns cmd's code; no `set -e` — exit codes are
# handled explicitly so the wdio test result is what this script returns.
set -o pipefail

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
pushd "$SCRIPTDIR" &> /dev/null

# Defaults
BUILD_MODE=""
CARGO_FEATURES=""
TESTS=""
DEBUG_MODE=""

# Parse arguments
for arg in "$@"; do
  case "$arg" in

    release)
      BUILD_MODE="release"
      ;;
    --release-build)
      BUILD_MODE="release"
      ;;
    --tests=*)
      TESTS="${arg#--tests=}"
      ;;
    --verbose-tracing)
      CARGO_FEATURES="--features veilid-core/verbose-tracing"
      ;;
    --debug)
      DEBUG_MODE=1
      ;;
    --help|-h)
      echo "Usage: $0 [options] [-- wdio args...]"
      echo ""
      echo "Options:"
      echo "  --release-build     Build in release mode (default: dev)"
      echo "  --tests=LIST        Comma-separated list of test groups to run."
      echo "                      If omitted, all tests are run."
      echo "                      Available groups:"
      echo "                        client         - veilidClient tests"
      echo "                        crypto         - veilidCrypto tests"
      echo "                        table          - VeilidTable tests"
      echo "                        routing        - VeilidRoutingContext tests"
      echo "                        dht            - DHT kitchen sink tests"
      echo "                        transactions   - DHT transaction tests"
      echo "  --verbose-tracing   Enable verbose-tracing cargo feature"
      echo "  --debug             Set DEBUG=1 environment variable"
      echo "  -- [args]           Pass remaining arguments to wdio"
      echo ""
      echo "Examples:"
      echo "  $0                                    # Run all tests"
      echo "  $0 --tests=transactions --debug       # Run DHT transaction tests with debugging output"
      echo "  $0 --tests=client,crypto              # Run client and crypto tests"
      echo "  $0 --release-build --tests=routing    # Release build, routing tests only"
      popd &> /dev/null
      exit 0
      ;;
    --)
      # Stop parsing arguments if -- is encountered
      shift
      break 
      ;;
    --*)
      echo "Unknown argument: $arg"
      exit 1
      ;;
  esac
  shift
done

# Build wasm into an npm package, output into ./pkg
./wasm_build_js.sh $BUILD_MODE $CARGO_FEATURES $CARGO_OPTIONS || { echo "wasm build failed" >&2; exit 1; }

# Install test deps and run test suite
cd tests || exit 1
npm install || { echo "npm install failed" >&2; exit 1; }
original_tmpdir=$TMPDIR
mkdir -p ~/tmp
export TMPDIR=~/tmp

# Build wdio arguments based on --tests selection
WDIO_ARGS=()

if [[ -n "$TESTS" ]]; then
  SPEC_ARGS=()
  GREP_PATTERNS=()

  IFS=',' read -ra TEST_LIST <<< "$TESTS"
  for test in "${TEST_LIST[@]}"; do
    case "$test" in
      client)
        SPEC_ARGS+=("./src/veilidClient.test.ts")
        ;;
      crypto)
        SPEC_ARGS+=("./src/veilidCrypto.test.ts")
        ;;
      table)
        SPEC_ARGS+=("./src/VeilidTable.test.ts")
        ;;
      routing)
        SPEC_ARGS+=("./src/VeilidRoutingContext.test.ts")
        ;;
      dht)
        SPEC_ARGS+=("./src/VeilidRoutingContext.test.ts")
        GREP_PATTERNS+=("DHT kitchen sink")
        ;;
      transactions)
        SPEC_ARGS+=("./src/VeilidRoutingContext.test.ts")
        GREP_PATTERNS+=("DHT transactions")
        ;;
      *)
        echo "Unknown test group: $test" >&2
        echo "Available groups: client, crypto, table, routing, dht, transactions" >&2
        exit 1
        ;;
    esac
  done

  # Deduplicate spec files
  UNIQUE_SPECS=($(printf '%s\n' "${SPEC_ARGS[@]}" | sort -u))
  for spec in "${UNIQUE_SPECS[@]}"; do
    WDIO_ARGS+=("--spec" "$spec")
  done

  # Combine grep patterns with | for OR matching
  if [[ ${#GREP_PATTERNS[@]} -gt 0 ]]; then
    GREP_COMBINED=$(IFS='|'; echo "${GREP_PATTERNS[*]}")
    WDIO_ARGS+=("--mochaOpts.grep" "$GREP_COMBINED")
  fi
fi

# Set DEBUG if requested
if [[ -n "$DEBUG_MODE" ]]; then
  export DEBUG=1
fi

# macOS quarantines a freshly-installed/updated chromedriver, which blocks wdio from launching it
if [[ "$(uname)" == "Darwin" ]] && command -v chromedriver >/dev/null; then
  xattr -d com.apple.quarantine "$(command -v chromedriver)" 2>/dev/null || true
fi

WDIO_HEADLESS=true npx wdio run ./wdio.conf.ts "${WDIO_ARGS[@]}" $@
status=$?

export TMPDIR=$original_tmpdir

popd &> /dev/null
exit $status
