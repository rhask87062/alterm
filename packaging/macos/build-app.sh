#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../common/metadata.sh
source "${SCRIPT_DIR}/../common/metadata.sh"

DIST_DIR="$(dist_dir)"
APP_DIR="${DIST_DIR}/${APP_NAME}.app"
CONTENTS_DIR="${APP_DIR}/Contents"
MACOS_DIR="${CONTENTS_DIR}/MacOS"
RESOURCES_DIR="${CONTENTS_DIR}/Resources"
PLIST_PATH="${CONTENTS_DIR}/Info.plist"

cd "${REPO_ROOT}"

echo "Building release binary..."
cargo build --release --package "${PACKAGE_NAME}"

echo "Creating macOS app bundle..."
rm -rf "${APP_DIR}"
mkdir -p "${MACOS_DIR}" "${RESOURCES_DIR}"

install -Dm755 "target/release/${BINARY_NAME}" "${MACOS_DIR}/${BINARY_NAME}"
install -Dm644 "config/default.toml" "${RESOURCES_DIR}/config.toml.example"
install -Dm644 "config/hooks.lua.example" "${RESOURCES_DIR}/hooks.lua.example"
install -Dm644 "README.md" "${RESOURCES_DIR}/README.txt"

sed \
    -e "s/__APP_NAME__/${APP_NAME}/g" \
    -e "s/__BINARY_NAME__/${BINARY_NAME}/g" \
    -e "s/__BUNDLE_ID__/${BUNDLE_ID}/g" \
    -e "s/__VERSION__/${VERSION}/g" \
    "${SCRIPT_DIR}/Info.plist.template" > "${PLIST_PATH}"

echo "App bundle created: ${APP_DIR}"
