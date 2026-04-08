#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../../common/metadata.sh
source "${SCRIPT_DIR}/../../common/metadata.sh"

ARCH="$(linux_deb_arch)"
PKG_NAME="${PACKAGE_NAME}_${VERSION}_${ARCH}"
BUILD_DIR="$(mktemp -d)"
PKG_ROOT="${BUILD_DIR}/${PKG_NAME}"
DIST_DIR="$(dist_dir)"

echo "Building release..."
cargo build --release --package "${PACKAGE_NAME}"

echo "Creating package structure..."
mkdir -p "${PKG_ROOT}/DEBIAN"
mkdir -p "${PKG_ROOT}/usr/bin"
mkdir -p "${PKG_ROOT}/usr/share/applications"
mkdir -p "${PKG_ROOT}/usr/share/alterm"
mkdir -p "${PKG_ROOT}/usr/share/icons/hicolor/256x256/apps"

sed \
    -e "s/__VERSION__/${VERSION}/g" \
    -e "s/__ARCH__/${ARCH}/g" \
    "${SCRIPT_DIR}/control" > "${PKG_ROOT}/DEBIAN/control"
install -Dm755 "${REPO_ROOT}/target/release/${BINARY_NAME}" "${PKG_ROOT}/usr/bin/${BINARY_NAME}"
install -Dm644 "${REPO_ROOT}/packaging/linux/alterm.desktop" "${PKG_ROOT}/usr/share/applications/alterm.desktop"
install -Dm644 "${REPO_ROOT}/config/default.toml" "${PKG_ROOT}/usr/share/alterm/config.toml.example"
install -Dm644 "${REPO_ROOT}/config/hooks.lua.example" "${PKG_ROOT}/usr/share/alterm/hooks.lua.example"
if [ -f "${LINUX_ICON_PATH}" ]; then
    install -Dm644 "${LINUX_ICON_PATH}" "${PKG_ROOT}/usr/share/icons/hicolor/256x256/apps/alterm.png"
fi

echo "Building .deb..."
dpkg-deb --build "${PKG_ROOT}" "${DIST_DIR}/${PKG_NAME}.deb"

echo "Package created: ${DIST_DIR}/${PKG_NAME}.deb"
rm -rf "${BUILD_DIR}"
