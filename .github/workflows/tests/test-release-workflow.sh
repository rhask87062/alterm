#!/bin/bash
# Structural checks on release.yml. These catch the mistakes that would
# otherwise only surface as a broken public download: a missing alias, a
# release that publishes before all platforms built, or a draft/prerelease
# flag that makes releases/latest/download/ resolve to nothing.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
WF="${REPO_ROOT}/.github/workflows/release.yml"
fail=0

check() {
    if eval "$2" >/dev/null 2>&1; then
        echo "  ok   $1"
    else
        echo "  FAIL $1" >&2
        fail=1
    fi
}

[ -f "${WF}" ] || { echo "FAIL: ${WF} does not exist" >&2; exit 1; }

echo "release.yml is valid YAML:"
check "parses" "python3 -c \"import yaml,sys; yaml.safe_load(open('${WF}'))\""

echo "triggers:"
check "tag trigger on v*"   "grep -q \"tags:\" '${WF}' && grep -q \"'v\\*'\" '${WF}'"
check "workflow_dispatch"   "grep -q 'workflow_dispatch' '${WF}'"

echo "all five alias filenames appear literally:"
for f in alterm-linux-amd64.deb alterm-linux-amd64.tar.gz \
         alterm-macos-arm64.pkg alterm-macos-x86_64.pkg \
         alterm-windows-x64-setup.exe; do
    check "${f}" "grep -q '${f}' '${WF}'"
done

echo "runners:"
for r in "ubuntu-latest" "macos-latest" "macos-13" "windows-latest"; do
    check "${r}" "grep -q '${r}' '${WF}'"
done

echo "safety properties:"
check "release job needs every build job" \
    "python3 -c \"
import yaml
wf = yaml.safe_load(open('${WF}'))
needs = wf['jobs']['release']['needs']
assert set(['linux','macos-arm','macos-x64','windows']).issubset(set(needs)), needs
\""
check "release is not a draft"      "grep -q 'draft: false' '${WF}'"
check "release is not a prerelease" "grep -q 'prerelease: false' '${WF}'"
check "version guard reads alterm/Cargo.toml" \
    "grep -q 'alterm/Cargo.toml' '${WF}'"
check "publishes only on a tag" \
    "python3 -c \"
import yaml
wf = yaml.safe_load(open('${WF}'))
cond = wf['jobs']['release'].get('if','')
assert 'tag' in cond, cond
\""
check "verifies download URLs after publish" \
    "grep -q 'releases/latest/download' '${WF}'"

DEPLOY="${REPO_ROOT}/.github/workflows/deploy-site.yml"
echo "deploy-site.yml runs the download checks:"
check "parses"           "python3 -c \"import yaml; yaml.safe_load(open('${DEPLOY}'))\""
check "check:downloads"  "grep -q 'check:downloads' '${DEPLOY}'"
check "check:page"       "grep -q 'check:page' '${DEPLOY}'"
check "check:copy"       "grep -q 'check:copy' '${DEPLOY}'"

[ "${fail}" -eq 0 ] && echo "PASS: release and deploy workflow structure is sound"
exit "${fail}"
