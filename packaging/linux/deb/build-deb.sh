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

install -Dm755 "${REPO_ROOT}/target/release/${BINARY_NAME}" "${PKG_ROOT}/usr/bin/${BINARY_NAME}"

# Derive Depends from the binary rather than hand-maintaining a list.
#
# The hand-written list shipped in 0.5.0 was both wrong and incomplete: it
# named `libgtk-3-0`, which Ubuntu's 64-bit time_t transition renamed to
# `libgtk-3-0t64`, so apt refused to install the package at all; and it omitted
# glib, libsoup, javascriptcoregtk and openssl, which only went unnoticed
# because those were already present on the machines that tried it.
#
# dpkg-shlibdeps reads the ELF's NEEDED entries and resolves each soname to the
# package that owns it on the build system, with proper version constraints. It
# cannot drift from what the binary actually links.
echo "Computing dependencies with dpkg-shlibdeps..."
SHLIBS_DIR="${BUILD_DIR}/shlibdeps"
mkdir -p "${SHLIBS_DIR}/debian"
# dpkg-shlibdeps insists on a debian/control next to where it runs.
printf 'Source: %s\n\nPackage: %s\nArchitecture: %s\nDepends: ${shlibs:Depends}\n' \
    "${PACKAGE_NAME}" "${PACKAGE_NAME}" "${ARCH}" > "${SHLIBS_DIR}/debian/control"

DEPENDS="$(cd "${SHLIBS_DIR}" && dpkg-shlibdeps -O --ignore-missing-info \
    "${PKG_ROOT}/usr/bin/${BINARY_NAME}" 2>/dev/null | sed 's/^shlibs:Depends=//')"

if [ -z "${DEPENDS}" ]; then
    echo "ERROR: dpkg-shlibdeps produced no dependencies." >&2
    echo "       A GTK/WebKit binary must depend on something; shipping an" >&2
    echo "       empty Depends would produce a package that installs and then" >&2
    echo "       fails to start. Install dpkg-dev and the -dev packages for" >&2
    echo "       GTK and WebKit, then rebuild." >&2
    exit 1
fi
echo "  Depends: ${DEPENDS}"

sed \
    -e "s/__VERSION__/${VERSION}/g" \
    -e "s/__ARCH__/${ARCH}/g" \
    -e "s|__DEPENDS__|${DEPENDS}|g" \
    "${SCRIPT_DIR}/control" > "${PKG_ROOT}/DEBIAN/control"
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
