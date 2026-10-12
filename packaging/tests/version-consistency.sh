#!/bin/bash
# Asserts the three version declarations that must agree all say the same
# thing. They are read by different consumers: metadata.sh (artifact names),
# Cargo's workspace inheritance, and the website's download page.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
fail=0

workspace="$(sed -n 's/^version = "\(.*\)"/\1/p' "${REPO_ROOT}/Cargo.toml" | head -n1)"
binary="$(sed -n 's/^version = "\(.*\)"/\1/p' "${REPO_ROOT}/alterm/Cargo.toml" | head -n1)"
site="$(sed -n 's/.*version: "\(.*\)",.*/\1/p' "${REPO_ROOT}/website/src/lib/site.ts" | head -n1)"

echo "workspace Cargo.toml : ${workspace}"
echo "alterm/Cargo.toml    : ${binary}"
echo "website site.ts      : ${site}"

expected="0.5.0"
for pair in "workspace:${workspace}" "binary:${binary}" "site:${site}"; do
    name="${pair%%:*}"; value="${pair#*:}"
    if [ "${value}" != "${expected}" ]; then
        echo "FAIL: ${name} is '${value}', expected '${expected}'" >&2
        fail=1
    fi
done

# metadata.sh is what actually names the artifacts — check it agrees.
metadata_version="$(cd "${REPO_ROOT}" && source packaging/common/metadata.sh && echo "${VERSION}")"
echo "metadata.sh VERSION  : ${metadata_version}"
if [ "${metadata_version}" != "${expected}" ]; then
    echo "FAIL: metadata.sh reports '${metadata_version}', expected '${expected}'" >&2
    fail=1
fi

[ "${fail}" -eq 0 ] && echo "PASS: all version declarations agree on ${expected}"
exit "${fail}"
