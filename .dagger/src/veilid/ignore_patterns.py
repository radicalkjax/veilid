"""Pre-upload filter for the module's `Directory` arguments.

Dagger has no way for a function argument to honor `.gitignore`: neither the `--source <path>`
CLI flag nor a `+defaultPath` argument sets `CopyFilter.Gitignore`, and there is no `+gitignore`
pragma. An `Ignore` list is the only pre-upload filter on the `--source` path, so without one
`dagger call --source .` uploads `target/` and every other build-artifact tree.

Prefer `dagger -c '<fn> $(host | directory --gitignore .)'`, which filters from the repo's
`.gitignore` files host-side and is always fresh. This list is the guard for the `--source .`
form, so it only has to catch the big artifact trees, not mirror `.gitignore`. It stays an
ignore list rather than an allowlist: a newly tracked file is uploaded without touching it.

No tracked file lives under any of these directory names; check with `git ls-files` before
adding one.
"""

DIRECTORY_IGNORE_PATTERNS: list[str] = [
    # committed, but the module runtime supplies it; main.py re-adds versions.env
    ".dagger",
    "**/target",
    "**/build",
    "**/.build",
    "**/dist",
    "**/.dart_tool",
    "**/.cxx",
    "**/.gradle",
    "**/node_modules",
    "**/.venv",
    "**/pkg",
]
