#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../common/metadata.sh
source "${SCRIPT_DIR}/../common/metadata.sh"

ARCHIVE_ROOT="$(mktemp -d)"
STAGE_DIR="${ARCHIVE_ROOT}/${PACKAGE_NAME}-${VERSION}-linux-$(linux_deb_arch)"
DIST_DIR="$(dist_dir)"

cleanup() {
    rm -rf "${ARCHIVE_ROOT}"
}
trap cleanup EXIT

cd "${REPO_ROOT}"

echo "Building release binary..."
cargo build --release --package "${PACKAGE_NAME}"

echo "Staging portable Linux bundle..."
mkdir -p "${STAGE_DIR}/bin"
mkdir -p "${STAGE_DIR}/share/applications"
mkdir -p "${STAGE_DIR}/share/alterm"
mkdir -p "${STAGE_DIR}/share/icons/hicolor/256x256/apps"

install -Dm755 "target/release/${BINARY_NAME}" "${STAGE_DIR}/bin/${BINARY_NAME}"
install -Dm644 "packaging/linux/alterm.desktop" "${STAGE_DIR}/share/applications/alterm.desktop"
install -Dm644 "config/default.toml" "${STAGE_DIR}/share/alterm/config.toml.example"
install -Dm644 "config/hooks.lua.example" "${STAGE_DIR}/share/alterm/hooks.lua.example"
install -Dm644 "README.md" "${STAGE_DIR}/README.md"
if [ -f "${LINUX_ICON_PATH}" ]; then
    install -Dm644 "${LINUX_ICON_PATH}" "${STAGE_DIR}/share/icons/hicolor/256x256/apps/alterm.png"
fi

ARCHIVE_PATH="${DIST_DIR}/${PACKAGE_NAME}-${VERSION}-linux-$(linux_deb_arch).tar.gz"
tar -C "${ARCHIVE_ROOT}" -czf "${ARCHIVE_PATH}" "$(basename "${STAGE_DIR}")"

echo "Portable archive created: ${ARCHIVE_PATH}"
