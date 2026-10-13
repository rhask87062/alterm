#!/bin/bash
# Asserts every dependency a built .deb declares is actually installable.
#
# This is the exact failure users hit with v0.5.0. Its control file was
# hand-written and declared `libgtk-3-0`, a name Ubuntu's 64-bit time_t
# transition retired in favour of `libgtk-3-0t64`, so apt refused the package:
#
#   alterm: Depends: libgtk-3-0 but it is not installable
#
# It also silently omitted four other required packages, which is why the
# build now derives Depends with dpkg-shlibdeps instead of hand-maintaining
# them. This test guards the result.
#
# Deliberately does NOT try to reverse-map linked libraries to owning packages.
# `dpkg -S` returns whichever package first claims a soname, which on a real
# machine can be a vendored copy under /opt rather than the system library —
# that produced confident nonsense when tried. dpkg-shlibdeps does that job
# properly at build time; this checks its output is satisfiable.
#
# Usage: deb-dependencies.sh <path-to-deb>
set -uo pipefail

DEB="${1:-}"
[ -n "${DEB}" ] && [ -f "${DEB}" ] || { echo "usage: $0 <path-to-deb>" >&2; exit 2; }
command -v dpkg-deb  >/dev/null || { echo "SKIP: dpkg-deb not available"; exit 0; }
command -v apt-cache >/dev/null || { echo "SKIP: apt-cache not available (not a Debian system)"; exit 0; }

depends="$(dpkg-deb -f "${DEB}" Depends 2>/dev/null)"
echo "Declared Depends:"
echo "  ${depends:-<none>}"
echo

if [ -z "${depends}" ]; then
    echo "FAIL: the package declares no Depends at all." >&2
    echo "      A GUI binary linking GTK and WebKit cannot have zero dependencies;" >&2
    echo "      this means dpkg-shlibdeps did not run or its output was dropped." >&2
    exit 1
fi

# A hand-written list is how this broke. Real shlibdeps output for this binary
# names six-plus packages with version constraints, so a suspiciously short
# list is worth flagging even when every entry happens to resolve.
clause_count="$(echo "${depends}" | tr ',' '\n' | grep -c '[^[:space:]]')"
echo "clause count: ${clause_count}"
if [ "${clause_count}" -lt 5 ]; then
    echo "  warn: fewer clauses than expected for a GTK/WebKit binary — check" \
         "that dpkg-shlibdeps actually ran" >&2
fi
echo

fail=0
echo "every declared dependency resolves:"
# Normalise to one clause per line, then iterate without a pipe so the failure
# counter survives (a pipeline would run the loop in a subshell).
mapfile -t clauses < <(echo "${depends}" | tr ',' '\n' | sed 's/^[[:space:]]*//;s/[[:space:]]*$//' | grep '[^[:space:]]')

for clause in "${clauses[@]}"; do
    satisfied=0
    # "a (>= 1) | b" — any branch resolving is enough; that is how t64 renames
    # are bridged for older releases.
    alts="$(echo "${clause}" | tr '|' '\n')"
    while IFS= read -r alt; do
        name="$(echo "${alt}" | sed 's/^[[:space:]]*//' | cut -d' ' -f1)"
        [ -n "${name}" ] || continue
        # Capture first, then match. Piping straight into `grep -q` makes grep
        # exit on the first hit, which SIGPIPEs apt-cache (141) and — under
        # `pipefail` — reports the whole pipeline as failed even though the
        # package resolved fine.
        policy="$(apt-cache policy "${name}" 2>/dev/null)"
        if printf '%s\n' "${policy}" | grep -q 'Candidate: [^(]'; then
            satisfied=1
            break
        fi
    done <<< "${alts}"

    if [ "${satisfied}" -eq 1 ]; then
        echo "  ok   ${clause}"
    else
        echo "  FAIL ${clause} — not installable on this system" >&2
        fail=1
    fi
done

echo
if [ "${fail}" -eq 0 ]; then
    echo "PASS: all ${clause_count} declared dependencies are installable"
else
    echo "FAIL: the package cannot be installed here" >&2
fi
exit "${fail}"
