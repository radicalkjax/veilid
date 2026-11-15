# Puts the CI-pinned Node major first on PATH. Source it before any npm call in a gate: npm writes
# the lock for its own major, so a host off the pin regenerates one CI's npm ci rejects. An nvm or
# Homebrew install of the pinned major is used when the default node is another one.
# The CI image carries the pins at /etc instead of the repo's .dagger
versions_env="$(dirname "${BASH_SOURCE[0]}")/../.dagger/versions.env"
if [ ! -f "$versions_env" ]; then
    versions_env=/etc/veilid-versions.env
fi
NODE_MAJOR="$(. "$versions_env" && echo "$NODE_MAJOR")"
node_major() { node --version 2>/dev/null | sed -E 's/^v([0-9]+).*/\1/'; }
if [ "$(node_major)" != "$NODE_MAJOR" ]; then
    pinned_bin="$( (ls -d "${NVM_DIR:-$HOME/.nvm}"/versions/node/v"$NODE_MAJOR".*/bin /opt/homebrew/opt/node@"$NODE_MAJOR"/bin /usr/local/opt/node@"$NODE_MAJOR"/bin 2>/dev/null || true) | sort -V | tail -1)"
    if [ -n "$pinned_bin" ]; then
        export PATH="$pinned_bin:$PATH"
    fi
fi
if [ "$(node_major)" != "$NODE_MAJOR" ]; then
    echo "node: $(node --version 2>/dev/null || echo missing) is not the CI-pinned major $NODE_MAJOR. Install it with:" >&2
    echo "  nvm install $NODE_MAJOR   (or: brew install node@$NODE_MAJOR)" >&2
    exit 1
fi
echo "Using node $(node --version) (CI pin $NODE_MAJOR)."
