#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../common/metadata.sh
source "${SCRIPT_DIR}/../common/metadata.sh"

if ! command -v pkgbuild >/dev/null 2>&1; then
    echo "pkgbuild is required to create a macOS installer package." >&2
    exit 1
fi

"${SCRIPT_DIR}/build-app.sh"

DIST_DIR="$(dist_dir)"
ROOT_DIR="$(mktemp -d)"
PKG_PATH="${DIST_DIR}/${PACKAGE_NAME}-${VERSION}-macos-$(macos_pkg_arch).pkg"

cleanup() {
    rm -rf "${ROOT_DIR}"
}
trap cleanup EXIT

mkdir -p "${ROOT_DIR}/Applications"
cp -R "${DIST_DIR}/${APP_NAME}.app" "${ROOT_DIR}/Applications/${APP_NAME}.app"

pkgbuild \
    --root "${ROOT_DIR}" \
    --identifier "${BUNDLE_ID}" \
    --version "${VERSION}" \
    --install-location "/" \
    "${PKG_PATH}"

echo "Installer created: ${PKG_PATH}"
