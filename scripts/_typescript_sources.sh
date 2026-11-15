# Shared TypeScript/JavaScript source globs and prettier invocation. Source it from the repo root.
#
# One list so scripts/fmt.sh and scripts/_check_format_typescript.sh can never disagree about what
# is checked. veilid-wasm/tests owns the pinned prettier (its package-lock.json); the config and
# ignore file sit in veilid-wasm/ so one setting covers the hand-written js/, the tests and the
# example. veilid-wasm/.prettierignore excludes the generated output (pkg/, bundles, coverage).

TYPESCRIPT_SOURCE_GLOBS=(
    'js/**/*.{js,ts}'
    'tests/**/*.{ts,mts,cts,js,mjs,cjs}'
    'example/test-bench/**/*.{ts,tsx,js,mjs,cjs}'
)

PRETTIER=veilid-wasm/tests/node_modules/.bin/prettier
if [ ! -x "$PRETTIER" ]; then
    echo "typescript: prettier is not installed. Install it with:" >&2
    echo "  (cd veilid-wasm/tests && npm ci)" >&2
    exit 1
fi

# Runs from veilid-wasm/ so its .prettierrc and .prettierignore apply to every glob
typescript_prettier() {
    (cd veilid-wasm && tests/node_modules/.bin/prettier "$@" "${TYPESCRIPT_SOURCE_GLOBS[@]}")
}
