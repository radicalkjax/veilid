# The crate directories whose README.md is generated from the crate docs by cargo-sync-readme,
# found by their markers so a new crate joins the gate by carrying them.
# Sourced by scripts/check_readme_sync.sh and scripts/fmt.sh from the repository root.

readme_crates() {
    grep -l "cargo-sync-readme start" -- */README.md 2>/dev/null | xargs -n1 dirname | sort
}
