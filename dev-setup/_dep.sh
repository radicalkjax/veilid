#!/bin/bash
# Shared dependency check/install engine for the veilid + veilidchat dev-setup
# scripts. Source it (don't run it). Platform scripts declare each dependency with
# the dep_* helpers, then call dep_summary.
#
# Modes:
#   default              check only — report each dep's status, exit non-zero if
#                        anything is missing/outdated.
#   --install            ensure MINIMUMS: install missing deps and bring below-minimum
#                        deps up to the required version. Deps that already meet their
#                        constraint are left untouched (no upgrade).
#   --upgrade, --update  ensure NEWEST: everything --install does, PLUS upgrade
#                        already-satisfied deps to the newest available version.
#
# Version policy:
#   dep_min   NAME GOT REQ INSTALL_FN     installed >= REQ is fine; below -> install.
#             Under --upgrade an already-ok dep is also refreshed to newest.
#   dep_exact NAME GOT REQ SBS INSTALL_FN installed must == REQ (a pin; never upgraded
#             past it). Newer + no side-by-side (SBS=0) -> CONFLICT: uninstall manually.
#   dep_have  NAME GOT HINT [INSTALL_FN]  presence only; under --upgrade, refreshed.
# An empty INSTALL_FN means "manual install" — reported, never auto-installed/upgraded.
# Installers must be upgrade-safe (package-manager upgrade, or a download that no-ops
# when the installed version already satisfies the requirement).

INSTALL=${INSTALL:-0}
UPGRADE=${UPGRADE:-0}
BUILDONLY=${BUILDONLY:-0}   # --build-only: only build-critical deps (skip dev/test tools)
_DEP_OK=(); _DEP_MISS=(); _DEP_OLD=(); _DEP_CONFLICT=()

dep_parse_args() {
    local a
    for a in "$@"; do
        case "$a" in
            --install) INSTALL=1 ;;
            --upgrade|--update) INSTALL=1; UPGRADE=1 ;;
            --build-only) BUILDONLY=1 ;;
        esac
    done
}
dep_help_requested() { local a; for a in "$@"; do case "$a" in -h|--help|-help) return 0;; esac; done; return 1; }

# ver_ge A B : true if version A >= version B. Portable (no GNU sort -V); compares
# dotted numeric fields, missing fields are 0, non-numeric suffixes are dropped.
ver_ge() {
    [ "$1" = "$2" ] && return 0
    local i x y oldifs=$IFS; local -a a b
    IFS=.; a=($1); b=($2); IFS=$oldifs
    for ((i = 0; i < ${#a[@]} || i < ${#b[@]}; i++)); do
        x=${a[i]:-0}; y=${b[i]:-0}; x=${x%%[^0-9]*}; y=${y%%[^0-9]*}
        ((10#${x:-0} > 10#${y:-0})) && return 0
        ((10#${x:-0} < 10#${y:-0})) && return 1
    done
    return 0
}

# ver_of "<text>" : first numeric version token in a tool's --version output.
# Allows bare integers (e.g. binaryen "wasm-opt version 129") as well as dotted.
ver_of() { echo "$1" | grep -oE '[0-9]+(\.[0-9]+)*' | head -n1; }

_say() { printf '  %b%-7s%b %-22s %s\n' "$2" "$1" '\033[0m' "$3" "$4"; }
_run() { echo "    -> $*"; "$@"; }

# dep_targets TARGET... — one counted "rust targets" dependency, reported in the same
# aligned format as dep_*. Adds only the missing targets (so rustup's per-target
# "is up to date" spam never appears). CHECK mode reports missing as [old]; --install
# adds them. No-op-ish if rustup is absent.
dep_targets() {
    local name="rust targets" have t n=$#; local -a add=()
    if ! command -v rustup >/dev/null 2>&1; then
        _DEP_MISS+=("$name"); _say '[MISS]' '\033[31m' "$name" "rustup not found"; return 0
    fi
    have=$(rustup target list --installed 2>/dev/null)
    for t in "$@"; do printf '%s\n' "$have" | grep -qx "$t" || add+=("$t"); done
    if [ ${#add[@]} -eq 0 ]; then
        _DEP_OK+=("$name"); _say '[ok]' '\033[32m' "$name" "all $n present"
    elif [ "$INSTALL" = 1 ]; then
        _say '[add]' '\033[33m' "$name" "adding ${#add[@]}: ${add[*]}"
        # default toolchain (stable in full mode; the pinned RUST_VERSION under --prefix) — NOT a
        # hardcoded `stable`, which wouldn't exist in a contained prefix that pinned RUST_VERSION.
        if rustup target add "${add[@]}"; then _DEP_OK+=("$name")
        else _DEP_CONFLICT+=("$name"); _say '[fail]' '\033[31m' "$name" "rustup target add failed"; fi
    else
        _DEP_OLD+=("$name"); _say '[old]' '\033[33m' "$name" "${#add[@]} missing (re-run with --install)"
    fi
}

# _maybe_install LABEL INSTALL_FN... — run installer iff --install and fn is set.
_maybe_install() {
    local name=$1 fn=$2; shift 2
    [ "$INSTALL" = 1 ] || return 0
    [ -n "$fn" ] || { _say '[skip]' '\033[33m' "$name" "manual install required"; return 0; }
    _run "$fn" "$@" || { _DEP_CONFLICT+=("$name"); _say '[fail]' '\033[31m' "$name" "install failed"; }
}

# _maybe_upgrade NAME FN — under --upgrade only, refresh an already-satisfied dep to
# the newest version. No-op without --upgrade or for manual (empty fn) deps.
_maybe_upgrade() {
    local name=$1 fn=$2
    [ "$UPGRADE" = 1 ] && [ -n "$fn" ] || return 0
    echo "    -> $name: ensuring newest"; "$fn" || { _DEP_CONFLICT+=("$name"); _say '[fail]' '\033[31m' "$name" "upgrade failed"; }
}

dep_min() {  # NAME GOT REQ [INSTALL_FN]  (installer optional — omit for a report-only check)
    local name=$1 got; got=$(ver_of "$2"); local req=$3 fn=${4:-}
    if [ -z "$got" ]; then
        _DEP_MISS+=("$name"); _say '[MISS]' '\033[31m' "$name" "not found (need >= $req)"; _maybe_install "$name" "$fn"
    elif ver_ge "$got" "$req"; then
        _DEP_OK+=("$name"); _say '[ok]' '\033[32m' "$name" "$got (>= $req)"; _maybe_upgrade "$name" "$fn"
    else
        _DEP_OLD+=("$name"); _say '[old]' '\033[33m' "$name" "$got (need >= $req)"; _maybe_install "$name" "$fn"
    fi
}

dep_exact() {  # NAME GOT REQ SBS(0/1) [INSTALL_FN]
    local name=$1 got; got=$(ver_of "$2"); local req=$3 sbs=$4 fn=${5:-}
    if [ -z "$got" ]; then
        _DEP_MISS+=("$name"); _say '[MISS]' '\033[31m' "$name" "not found (need == $req)"; _maybe_install "$name" "$fn"
    elif [ "$got" = "$req" ]; then
        _DEP_OK+=("$name"); _say '[ok]' '\033[32m' "$name" "$got (== $req)"
    elif [ "$sbs" = 1 ]; then
        _DEP_OLD+=("$name"); _say '[diff]' '\033[33m' "$name" "$got, want $req (side-by-side)"; _maybe_install "$name" "$fn"
    elif ver_ge "$got" "$req"; then
        _DEP_CONFLICT+=("$name"); _say '[!!]' '\033[31m' "$name" "$got > required $req — uninstall it manually, then re-run"
    else
        _DEP_OLD+=("$name"); _say '[old]' '\033[33m' "$name" "$got (need == $req)"; _maybe_install "$name" "$fn"
    fi
}

dep_have() {  # NAME GOT HINT [INSTALL_FN]
    local name=$1 got=$2 hint=$3 fn=${4:-}
    if [ -n "$got" ]; then _DEP_OK+=("$name"); _say '[ok]' '\033[32m' "$name" "present"; _maybe_upgrade "$name" "$fn"
    else _DEP_MISS+=("$name"); _say '[MISS]' '\033[31m' "$name" "$hint"; _maybe_install "$name" "$fn"; fi
}

dep_summary() {
    echo
    printf 'Summary: %d ok, %d missing, %d outdated, %d conflict\n' \
        ${#_DEP_OK[@]} ${#_DEP_MISS[@]} ${#_DEP_OLD[@]} ${#_DEP_CONFLICT[@]}
    if [ ${#_DEP_CONFLICT[@]} -gt 0 ]; then
        echo "Manual action required (uninstall/fix, then re-run): ${_DEP_CONFLICT[*]}"
        return 2
    fi
    if [ "$INSTALL" != 1 ] && { [ ${#_DEP_MISS[@]} -gt 0 ] || [ ${#_DEP_OLD[@]} -gt 0 ]; }; then
        echo "Some dependencies are missing or out of date. Re-run with one of:"
        echo "    $0 --install     # install/repair to meet the minimum requirements"
        echo "    $0 --upgrade     # also upgrade everything to the newest versions"
        return 1
    fi
    return 0
}
