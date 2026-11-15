# Shared Python source roots and black invocation. Source it from the repo root.
#
# One list so scripts/fmt.sh and scripts/_check_format_python.sh can never disagree about what is
# checked. veilid-python owns the pinned black and mypy (its uv.lock) and the [tool.black]
# settings, so the other roots format with that same install and config instead of carrying
# their own.
#
# Not covered: scripts/earthly/secretsd (a vendored upstream project, kept as it came) and
# doc/security/poc (a frozen proof of concept).

PYTHON_SOURCE_ROOTS=(veilid-python .dagger/src scripts/cicd-python scripts/stats)

if ! command -v uv >/dev/null 2>&1; then
    echo "python: uv is not installed; run dev-setup/setup_<os>.sh --install" >&2
    exit 1
fi

# uv syncs veilid-python's dev group on first use, so the pinned black is always the one that runs
python_black() {
    uv run --project veilid-python black --config veilid-python/pyproject.toml "$@" \
        "${PYTHON_SOURCE_ROOTS[@]}"
}
