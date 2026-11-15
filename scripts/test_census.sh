#!/bin/bash
# Self-test for the census rules. A gate is only worth running if it fails on the code it
# was written to catch, so every case asserts a direction: an undeclared site fails, a
# declared one passes, a declaration below the site does not count, and a declaration
# cannot be borrowed by a second site.
#
# The fixtures are checked with the SHIPPED rule files, with only their `files:` scoping
# removed so they apply to the fixture tree. Testing a copy of the rule would prove
# nothing about the rule that actually runs.

set -uo pipefail

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
cd "$SCRIPTDIR/.." || exit 2

if ! command -v ast-grep > /dev/null 2>&1; then
    echo "ast-grep is not installed. Please install it with: cargo install ast-grep --locked"
    exit 1
fi

work="$(mktemp -d)" || exit 2
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/rules" "$work/src"
printf 'ruleDirs:\n  - rules\n' > "$work/sgconfig.yml"

# Copy each shipped rule, dropping its `files:` list so it applies to the fixtures.
python3 - "$work/rules" scripts/census/rules/*.yml <<'PY'
import re, sys, os, shutil
dest, srcs = sys.argv[1], sys.argv[2:]
for src in srcs:
    text = open(src, encoding="utf-8").read()
    # Drop a top-level `files:` block: the key and its indented list items.
    text = re.sub(r'^files:\n(?:[ \t]+-.*\n)+', '', text, flags=re.M)
    open(os.path.join(dest, os.path.basename(src)), "w", encoding="utf-8").write(text)
PY

failures=0

fixture() { cat > "$work/src/fixture.rs"; }

# expect <pass|fail> <rule-id> <name>
expect() {
    local want="$1" rule="$2" name="$3" out rc
    out="$(cd "$work" && ast-grep scan --filter "^$rule\$" 2>&1)"
    rc=$?
    if [ "$want" = "pass" ] && [ $rc -ne 0 ]; then
        echo "FAIL: $name - expected no finding, got:" >&2
        echo "$out" | sed 's/^/    /' >&2
        failures=$((failures + 1))
    elif [ "$want" = "fail" ] && [ $rc -eq 0 ]; then
        echo "FAIL: $name - expected a finding, got none" >&2
        failures=$((failures + 1))
    fi
}

# expect_count <rule-id> <n> <name>
# For rules whose severity is below error: those never change the exit code, so the
# assertion has to be on how many sites they report.
expect_count() {
    local rule="$1" want="$2" name="$3" got
    got="$(cd "$work" && ast-grep scan --filter "^$rule\$" --json=compact 2>/dev/null \
        | python3 -c 'import json,sys; print(len(json.load(sys.stdin)))')"
    if [ "$got" != "$want" ]; then
        echo "FAIL: $name - expected $want site(s), got $got" >&2
        failures=$((failures + 1))
    fi
}

###########################################################################
# unsafe-safety

fixture <<'EOF'
fn f() {
    unsafe { libc::getpid() };
}
EOF
expect fail unsafe-safety "an undeclared unsafe block is caught"

fixture <<'EOF'
fn f() {
    // SAFETY: the pid is always readable
    unsafe { libc::getpid() };
}
EOF
expect pass unsafe-safety "a declaration directly above is accepted"

fixture <<'EOF'
fn f() {
    unsafe { libc::getpid() };
    // SAFETY: below the site, must not count
}
EOF
expect fail unsafe-safety "a declaration below the site does not count"

# The case the grep gates needed a line window for: a formatter parks the comment on its
# own line and pushes the block down, or splits the block across lines.
fixture <<'EOF'
fn f() {
    // SAFETY: this argument runs to a second line, which is normal for a real
    // safety argument and must not stop the declaration counting
    let r = unsafe {
        libc::sendto(
            fd,
            buf,
            len,
            0,
            addr,
            addrlen,
        )
    };
}
EOF
expect pass unsafe-safety "a declaration survives a rewrap and a multi-line comment"

fixture <<'EOF'
fn f() {
    // SAFETY: serves the first site only
    unsafe { libc::getpid() };
    unsafe { libc::getppid() };
}
EOF
expect fail unsafe-safety "a declaration cannot be borrowed by a second site"

fixture <<'EOF'
#[cfg(test)]
mod tests {
    #[test]
    fn t() {
        unsafe { libc::getpid() };
    }
}
EOF
expect pass unsafe-safety "test modules are exempt"

# The backstop: the population rule must see the sites the violation rule judges. If the
# matcher silently stops working this is what notices.
fixture <<'EOF'
fn f() {
    // SAFETY: declared, so unsafe-safety is silent here
    unsafe { libc::getpid() };
}
EOF
expect_count unsafe-safety-sites 1 "the population rule counts a site even when it is declared"

###########################################################################
# panic-census

fixture <<'EOF'
fn f() {
    panic!("boom");
}
EOF
expect fail panic-census "an undeclared panic is caught"

fixture <<'EOF'
fn f() {
    // PANIC-CENSUS: infallible serde cannot fail for these derive types
    panic!("boom");
}
EOF
expect pass panic-census "a declaration with a verb and a justification is accepted"

fixture <<'EOF'
fn f() {
    // PANIC-CENSUS: infallible
    panic!("boom");
}
EOF
expect fail panic-census "a verb with no justification is rejected"

fixture <<'EOF'
fn f() {
    // PANIC-CENSUS: reminted eventually
    panic!("boom");
}
EOF
expect fail panic-census "a verb outside the vocabulary is rejected"

fixture <<'EOF'
fn f() {
    debug_assert!(x, "invariant");
    todo!();
}
EOF
expect pass panic-census "debug_assert and todo are out of scope"

fixture <<'EOF'
fn f() {
    // PANIC-CENSUS: crash-test serves the first site only
    panic!("one");
    panic!("two");
}
EOF
expect fail panic-census "a declaration cannot be borrowed by a second site"

# The macro can sit several nodes deep inside the statement the declaration sits above:
# a closure, inside a call, inside a return. The declaration still covers it.
fixture <<'EOF'
fn f() -> Result<()> {
    // PANIC-CENSUS: provably-dead the non-Value arm carries no value to map
    return Ok(nr.map(|_| unreachable!()));
}
EOF
expect pass panic-census "a declaration covers a macro nested inside its own statement"

# ...but it stops at the enclosing block, so it cannot reach into a nested one.
fixture <<'EOF'
fn f() {
    // PANIC-CENSUS: crash-test this must not reach inside the loop
    for i in 0..3 {
        panic!("inside a nested block");
    }
}
EOF
expect fail panic-census "a declaration does not reach into a nested block"

fixture <<'EOF'
#[test]
fn t() {
    assert_eq!(a, b);
}
EOF
expect pass panic-census "a bare #[test] fn is exempt"

###########################################################################

if [ $failures -ne 0 ]; then
    echo "$failures census self-test case(s) failed" >&2
    exit 1
fi
echo "Census gate self-test OK."
