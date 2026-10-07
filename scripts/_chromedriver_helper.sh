#!/usr/bin/env bash
# Cross-platform chromedriver helper for flutter web integration tests.
#
# Source this file from a run_integration_tests_web.sh wrapper:
#   source "${VEILID_ROOT}/scripts/_chromedriver_helper.sh"
#   trap stop_chromedriver EXIT INT TERM
#   start_chromedriver
#   flutter drive -d chrome ...
#
# Functions:
#   start_chromedriver  - resolve a working chromedriver and start a fresh
#                         instance on port 4444. If another chromedriver is
#                         already listening on that port (stale from a prior
#                         run), it gets killed first. Captures the new PID.
#   stop_chromedriver   - kill the chromedriver pid we started (graceful
#                         SIGTERM, then SIGKILL if it doesn't exit promptly).
#                         Also kills the log poller and removes the headless
#                         Chrome wrapper file.
#
# Resolution order for the chromedriver binary:
#   1. $CHROMEDRIVER env var (if executable)
#   2. System chromedriver in PATH — used on Linux/Windows where the package
#      manager keeps it version-matched. Skipped on macOS where a stale
#      Homebrew chromedriver in PATH often mismatches the installed Chrome.
#   3. Cached download under ~/.cache/veilid/chromedriver/<CHROME_BUILD>/
#   4. Auto-download from https://googlechromelabs.github.io/chrome-for-testing/
#      (Google Chrome only; Chromium users must install chromedriver via
#      their package manager).

CHROMEDRIVER_PORT="${CHROMEDRIVER_PORT:-4444}"
_chromedriver_started_pid=""
_chromedriver_log_poller_pid=""
_chromedriver_binary=""
_headless_chrome_wrapper=""

_chromedriver_os() {
    case "$(uname -s 2>/dev/null)" in
        Darwin)            echo macos ;;
        Linux)             echo linux ;;
        MINGW*|MSYS*|CYGWIN*) echo windows ;;
        *)                 echo unknown ;;
    esac
}

_chromedriver_platform() {
    case "$(_chromedriver_os)" in
        macos)
            if [ "$(uname -m)" = "arm64" ]; then echo mac-arm64; else echo mac-x64; fi
            ;;
        linux)   echo linux64 ;;
        windows)
            if [ "${PROCESSOR_ARCHITECTURE:-}" = "x86" ] && [ -z "${PROCESSOR_ARCHITEW6432:-}" ]; then
                echo win32
            else
                echo win64
            fi
            ;;
        *) echo unknown ;;
    esac
}

_chromedriver_locate_chrome() {
    case "$(_chromedriver_os)" in
        macos)
            local bin="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
            if [ -x "$bin" ]; then echo "$bin"; return 0; fi
            return 1
            ;;
        linux)
            local c
            for c in google-chrome google-chrome-stable chromium-browser chromium; do
                if command -v "$c" >/dev/null 2>&1; then command -v "$c"; return 0; fi
            done
            return 1
            ;;
        windows)
            local c
            for c in \
                "/c/Program Files/Google/Chrome/Application/chrome.exe" \
                "/c/Program Files (x86)/Google/Chrome/Application/chrome.exe" \
                "${LOCALAPPDATA:-}/Google/Chrome/Application/chrome.exe"; do
                if [ -n "$c" ] && [ -x "$c" ]; then echo "$c"; return 0; fi
            done
            return 1
            ;;
    esac
    return 1
}

_chromedriver_chrome_is_chromium() {
    case "$1" in
        *chromium*|*Chromium*) return 0 ;;
        *) return 1 ;;
    esac
}

_chromedriver_install_hint() {
    case "$(_chromedriver_os)" in
        macos)
            echo "  Install Google Chrome from https://www.google.com/chrome/"
            echo "  Or install chromedriver manually and set CHROMEDRIVER=/path/to/chromedriver"
            ;;
        linux)
            echo "  Debian/Ubuntu: sudo apt install google-chrome-stable chromium-driver"
            echo "  Fedora:        sudo dnf install google-chrome-stable chromedriver"
            echo "  Arch:          sudo pacman -S chromium  (includes chromedriver)"
            echo "  Or set CHROMEDRIVER=/path/to/chromedriver"
            ;;
        windows)
            echo "  Install Google Chrome from https://www.google.com/chrome/"
            echo "  chromedriver:  choco install chromedriver  (or scoop install chromedriver)"
            echo "  Or set CHROMEDRIVER=/path/to/chromedriver.exe"
            ;;
        *)
            echo "  Install Google Chrome and chromedriver from https://googlechromelabs.github.io/chrome-for-testing/"
            ;;
    esac
}

_chromedriver_port_open() {
    curl -sf "http://localhost:${CHROMEDRIVER_PORT}/status" >/dev/null 2>&1
}

_chromedriver_wait_ready() {
    local deadline=$(( SECONDS + 15 ))
    while [ $SECONDS -lt $deadline ]; do
        if _chromedriver_port_open; then return 0; fi
        sleep 0.25
    done
    return 1
}

_chromedriver_cache_dir() {
    local build="$1"
    echo "${HOME}/.cache/veilid/chromedriver/${build}"
}

_chromedriver_download() {
    local build="$1" platform="$2" cache_bin="$3"

    if ! command -v python3 >/dev/null 2>&1; then
        echo "ERROR: python3 required to parse chrome-for-testing release JSON." >&2
        return 1
    fi
    if ! command -v curl >/dev/null 2>&1 || ! command -v unzip >/dev/null 2>&1; then
        echo "ERROR: curl and unzip required to download chromedriver." >&2
        return 1
    fi

    echo "Downloading chromedriver for Chrome build ${build} (${platform})..."

    local json_url="https://googlechromelabs.github.io/chrome-for-testing/latest-patch-versions-per-build-with-downloads.json"
    local json
    json="$(curl -fsSL "$json_url")" || { echo "ERROR: failed to fetch ${json_url}" >&2; return 1; }

    local url
    url="$(printf '%s' "$json" | python3 -c "
import sys, json
data = json.load(sys.stdin)
build = '$build'
platform = '$platform'
try:
    for entry in data['builds'][build]['downloads']['chromedriver']:
        if entry['platform'] == platform:
            print(entry['url'])
            sys.exit(0)
    print(f'no chromedriver for platform {platform}', file=sys.stderr); sys.exit(1)
except KeyError:
    print(f'no chromedriver for build {build}', file=sys.stderr); sys.exit(1)
")" || { echo "ERROR: chromedriver URL lookup failed." >&2; return 1; }

    local tmpdir rc=0
    tmpdir="$(mktemp -d)"

    if ! curl -fsSL "$url" -o "$tmpdir/chromedriver.zip"; then
        echo "ERROR: failed to download $url" >&2
        rc=1
    elif ! unzip -q "$tmpdir/chromedriver.zip" -d "$tmpdir"; then
        echo "ERROR: failed to unzip chromedriver" >&2
        rc=1
    else
        local extracted
        extracted="$(find "$tmpdir" \( -name chromedriver -o -name chromedriver.exe \) -type f | head -1)"
        if [ -z "$extracted" ]; then
            echo "ERROR: chromedriver binary not found in zip" >&2
            rc=1
        else
            mkdir -p "$(dirname "$cache_bin")"
            cp "$extracted" "$cache_bin"
            chmod +x "$cache_bin" 2>/dev/null || true
            echo "Installed: $("$cache_bin" --version 2>&1 | head -1)"
        fi
    fi

    rm -rf "$tmpdir"
    return "$rc"
}

# Strip macOS Gatekeeper quarantine so a cask/$CHROMEDRIVER-supplied binary can launch
# (curl+unzip downloads aren't quarantined, so this is a no-op there). No-op off macOS.
_chromedriver_unquarantine() {
    # Best-effort: removing a missing quarantine xattr returns non-zero, which under
    # the caller's `set -e` would abort the whole script before `return 0` — so guard
    # the fallible xattr with `|| true`.
    if [ "$(_chromedriver_os)" = macos ] && [ -n "$1" ]; then
        xattr -d com.apple.quarantine "$1" 2>/dev/null || true
    fi
    return 0
}

_chromedriver_resolve_binary() {
    if [ -n "${CHROMEDRIVER:-}" ] && [ -x "${CHROMEDRIVER:-}" ]; then
        _chromedriver_binary="$CHROMEDRIVER"
        echo "chromedriver: using \$CHROMEDRIVER = $_chromedriver_binary"
        return 0
    fi

    if [ "$(_chromedriver_os)" != "macos" ] && command -v chromedriver >/dev/null 2>&1; then
        _chromedriver_binary="$(command -v chromedriver)"
        echo "chromedriver: using system PATH binary $_chromedriver_binary"
        return 0
    fi

    local chrome
    chrome="$(_chromedriver_locate_chrome)" || {
        echo "ERROR: Chrome/Chromium not found." >&2
        _chromedriver_install_hint >&2
        return 1
    }
    echo "chromedriver: found browser $chrome"

    if _chromedriver_chrome_is_chromium "$chrome" && ! command -v chromedriver >/dev/null 2>&1; then
        echo "ERROR: Chromium detected but no chromedriver in PATH." >&2
        echo "  Google's CDN only has chromedriver for Google Chrome — install via package manager:" >&2
        _chromedriver_install_hint >&2
        return 1
    fi

    local version full_build platform cache_bin
    full_build="$("$chrome" --version 2>&1 | grep -oE '[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+' | head -1)"
    if [ -z "$full_build" ]; then
        echo "ERROR: could not parse Chrome version from '$("$chrome" --version 2>&1)'" >&2
        return 1
    fi
    version="$(echo "$full_build" | cut -d. -f1-3)"
    platform="$(_chromedriver_platform)"
    if [ "$platform" = "unknown" ]; then
        echo "ERROR: unsupported platform for chromedriver auto-download." >&2
        _chromedriver_install_hint >&2
        return 1
    fi

    local cache_dir
    cache_dir="$(_chromedriver_cache_dir "$version")"
    if [ "$(_chromedriver_os)" = "windows" ]; then
        cache_bin="$cache_dir/chromedriver.exe"
    else
        cache_bin="$cache_dir/chromedriver"
    fi

    if [ -x "$cache_bin" ]; then
        local cached_build
        cached_build="$("$cache_bin" --version 2>&1 | grep -oE '[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+' | head -1 | cut -d. -f1-3)"
        if [ "$cached_build" = "$version" ]; then
            _chromedriver_binary="$cache_bin"
            echo "chromedriver: using cached $cache_bin"
            return 0
        fi
    fi

    _chromedriver_download "$version" "$platform" "$cache_bin" || return 1
    _chromedriver_binary="$cache_bin"
    return 0
}

# Resolve a working chromedriver binary (downloading if needed) and print its
# path. Used by callers that don't want a port-bound server — wasm-bindgen-test
# launches its own chromedriver session and just needs the CHROMEDRIVER env var
# to point at a binary it can exec.
chromedriver_binary_path() {
    _chromedriver_resolve_binary >/dev/null || return 1
    _chromedriver_unquarantine "$_chromedriver_binary"
    printf '%s' "$_chromedriver_binary"
}

# Return the PID of whatever process is listening on $CHROMEDRIVER_PORT, or
# empty if nothing is. Uses lsof when available; falls back to ss on Linux.
_chromedriver_pid_on_port() {
    if command -v lsof >/dev/null 2>&1; then
        lsof -ti "tcp:${CHROMEDRIVER_PORT}" -sTCP:LISTEN 2>/dev/null | head -1
    elif command -v ss >/dev/null 2>&1; then
        ss -tlnpH "sport = :${CHROMEDRIVER_PORT}" 2>/dev/null \
            | grep -oE 'pid=[0-9]+' | head -1 | cut -d= -f2
    fi
}

# Kill any stale chromedriver listening on our port. Refuses to kill a
# non-chromedriver process (e.g. some other webdev tool happens to be on 4444);
# in that case the caller should set CHROMEDRIVER_PORT to a free port.
_chromedriver_reclaim_port() {
    local pid
    pid="$(_chromedriver_pid_on_port)"
    [ -z "$pid" ] && return 0

    local cmd
    cmd="$(ps -o comm= -p "$pid" 2>/dev/null | tr -d ' \t')"
    case "$cmd" in
        *chromedriver*)
            echo "chromedriver: killing stale instance (pid ${pid}) on port ${CHROMEDRIVER_PORT}"
            kill "$pid" 2>/dev/null || true
            local deadline=$(( SECONDS + 3 ))
            while [ $SECONDS -lt $deadline ]; do
                kill -0 "$pid" 2>/dev/null || return 0
                sleep 0.1
            done
            kill -9 "$pid" 2>/dev/null || true
            ;;
        *)
            echo "ERROR: port ${CHROMEDRIVER_PORT} is held by '${cmd:-unknown}' (pid ${pid}), not chromedriver." >&2
            echo "  Set CHROMEDRIVER_PORT=<free port> to use a different port." >&2
            return 1
            ;;
    esac
}

start_chromedriver() {
    # Always start our own — never inherit a stale instance whose WebDriver
    # sessions, log buffers, and Chrome processes belong to a prior failed run.
    _chromedriver_reclaim_port || return 1
    _chromedriver_resolve_binary || return 1
    _chromedriver_unquarantine "$_chromedriver_binary"

    echo "chromedriver: starting on port ${CHROMEDRIVER_PORT} ($("$_chromedriver_binary" --version 2>&1 | head -1))"
    "$_chromedriver_binary" --port="${CHROMEDRIVER_PORT}" --silent >/dev/null 2>&1 &
    _chromedriver_started_pid=$!
    echo "chromedriver: started pid ${_chromedriver_started_pid}"

    if ! _chromedriver_wait_ready; then
        echo "ERROR: chromedriver (pid ${_chromedriver_started_pid}) never started listening on port ${CHROMEDRIVER_PORT}." >&2
        echo "  Most common cause: chromedriver version doesn't match installed Chrome." >&2
        kill "${_chromedriver_started_pid}" 2>/dev/null || true
        wait "${_chromedriver_started_pid}" 2>/dev/null || true
        _chromedriver_started_pid=""
        return 1
    fi
    return 0
}

stop_chromedriver() {
    stop_chromedriver_log_poller
    if [ -n "${_chromedriver_started_pid}" ]; then
        echo "chromedriver: stopping pid ${_chromedriver_started_pid}"
        kill "${_chromedriver_started_pid}" 2>/dev/null || true
        local deadline=$(( SECONDS + 2 ))
        while [ $SECONDS -lt $deadline ]; do
            kill -0 "${_chromedriver_started_pid}" 2>/dev/null || break
            sleep 0.1
        done
        if kill -0 "${_chromedriver_started_pid}" 2>/dev/null; then
            kill -9 "${_chromedriver_started_pid}" 2>/dev/null || true
        fi
        wait "${_chromedriver_started_pid}" 2>/dev/null || true
        _chromedriver_started_pid=""
    fi
    if [ -n "${_headless_chrome_wrapper}" ] && [ -f "${_headless_chrome_wrapper}" ]; then
        rm -f "${_headless_chrome_wrapper}"
        _headless_chrome_wrapper=""
    fi
}

# Stream browser console messages from the active chromedriver WebDriver
# session to this process's stdout. Required for `-d web-server` mode, which
# otherwise leaves browser console output stranded inside Chrome. Uses
# ChromeDriver's legacy POST /session/{id}/log endpoint (active when flutter
# drive requests goog:loggingPrefs.browser: INFO during session creation).
start_chromedriver_log_poller() {
    (
        local session_id=""
        local deadline=$(( SECONDS + 120 ))
        while [ -z "$session_id" ] && [ $SECONDS -lt $deadline ]; do
            session_id="$(curl -sf "http://localhost:${CHROMEDRIVER_PORT}/sessions" 2>/dev/null \
                | python3 -c 'import sys, json
d = json.load(sys.stdin)
v = d.get("value") or []
print(v[0]["id"] if v else "")' 2>/dev/null)"
            [ -z "$session_id" ] && sleep 0.5
        done
        if [ -z "$session_id" ]; then
            echo "chromedriver log poller: no session appeared within timeout" >&2
            exit 0
        fi
        echo "chromedriver log poller: streaming logs from session ${session_id}"

        while true; do
            local body
            body="$(curl -sf -X POST "http://localhost:${CHROMEDRIVER_PORT}/session/${session_id}/log" \
                -H 'Content-Type: application/json' \
                -d '{"type":"browser"}' 2>/dev/null)" || break
            printf '%s' "$body" | python3 -c 'import sys, json
try:
    d = json.load(sys.stdin)
except Exception:
    sys.exit(0)
for e in d.get("value") or []:
    msg = e.get("message", "")
    print(msg)' 2>/dev/null
            sleep 0.5
        done
    ) &
    _chromedriver_log_poller_pid=$!
}

stop_chromedriver_log_poller() {
    if [ -n "${_chromedriver_log_poller_pid}" ]; then
        kill "${_chromedriver_log_poller_pid}" 2>/dev/null || true
        wait "${_chromedriver_log_poller_pid}" 2>/dev/null || true
        _chromedriver_log_poller_pid=""
    fi
}

# Generate a Chrome wrapper script that prepends --headless=new (and a fresh
# isolated user-data-dir) to every Chrome invocation, and export
# CHROME_EXECUTABLE pointing at it. Used with `flutter drive -d chrome` so the
# Chrome flutter_tools launches to host the test app runs headless while still
# proxying the browser console to flutter drive's stdout. Cleanup is handled
# by stop_chromedriver.
setup_headless_chrome_executable() {
    local chrome
    chrome="$(_chromedriver_locate_chrome)" || {
        echo "ERROR: Chrome not found." >&2
        _chromedriver_install_hint >&2
        return 1
    }

    _headless_chrome_wrapper="$(mktemp -t chrome_headless_XXXXXX)"
    cat > "${_headless_chrome_wrapper}" <<EOF
#!/usr/bin/env bash
exec "${chrome}" --headless=new --no-first-run --disable-gpu "\$@"
EOF
    chmod +x "${_headless_chrome_wrapper}"
    export CHROME_EXECUTABLE="${_headless_chrome_wrapper}"
    echo "chrome: headless wrapper at ${_headless_chrome_wrapper} -> ${chrome}"
}

# Run flutter drive, watching for the test-result marker. flutter drive on web
# routinely hangs after the result line; we kill its process group when seen.
# Returns 0 if "All tests passed!", 1 if "Some tests failed.", 2 if no marker.
run_flutter_drive_for_web_tests() {
    local fifo
    fifo="$(mktemp -u -t fdrive.XXXXXX)"
    mkfifo "$fifo" || { echo "ERROR: mkfifo failed" >&2; return 3; }

    # set -m puts the bg job in its own pgid (= pid) so we can kill -- -PID
    # to nuke the whole tree. disown silences job-control "Terminated" noise.
    set -m
    flutter drive "$@" >"$fifo" 2>&1 &
    local flutter_pid=$!
    set +m
    disown "$flutter_pid" 2>/dev/null || true
    exec 5<"$fifo"

    local result=2 line
    while IFS= read -r line <&5; do
        printf '%s\n' "$line"
        # Punctuation varies by mode: flutter prints "All tests passed!" in
        # debug and "All tests passed." in release/web-server — match either.
        case "$line" in
            *"All tests passed"*) result=0; break ;;
            *"Some tests failed"*) result=1; break ;;
        esac
    done
    exec 5<&-
    rm -f "$fifo"

    if [ "$result" -ne 2 ]; then
        local deadline=$(( SECONDS + 2 ))
        while [ $SECONDS -lt $deadline ]; do
            kill -0 "$flutter_pid" 2>/dev/null || break
            sleep 0.1
        done
    fi
    if kill -0 "$flutter_pid" 2>/dev/null; then
        echo "[runner] flutter drive hung after tests; terminating process group -${flutter_pid}" >&2
        kill -TERM -- "-${flutter_pid}" 2>/dev/null || true
        local deadline=$(( SECONDS + 3 ))
        while [ $SECONDS -lt $deadline ]; do
            kill -0 "$flutter_pid" 2>/dev/null || break
            sleep 0.1
        done
        if kill -0 "$flutter_pid" 2>/dev/null; then
            kill -KILL -- "-${flutter_pid}" 2>/dev/null || true
        fi
    fi
    while kill -0 "$flutter_pid" 2>/dev/null; do sleep 0.05; done

    [ "$result" -eq 2 ] && echo "[runner] WARNING: flutter drive ended without a test-result marker" >&2
    return "$result"
}
