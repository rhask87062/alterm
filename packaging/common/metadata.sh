#!/bin/bash
set -euo pipefail

PACKAGING_COMMON_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PACKAGING_ROOT="$(cd "${PACKAGING_COMMON_DIR}/.." && pwd)"
REPO_ROOT="$(cd "${PACKAGING_ROOT}/.." && pwd)"

APP_NAME="Alterm"
PACKAGE_NAME="alterm"
BINARY_NAME="alterm"
BUNDLE_ID="dev.alterm.app"
MAINTAINER_NAME="Russell Haskell"
MAINTAINER_EMAIL="rhask87062@gmail.com"
DESCRIPTION="GPU-accelerated terminal workspace with AI integration"

read_version() {
    sed -n 's/^version = "\(.*\)"/\1/p' "${REPO_ROOT}/alterm/Cargo.toml" | head -n1
}

VERSION="${VERSION:-$(read_version)}"

dist_dir() {
    local dir="${REPO_ROOT}/dist"
    mkdir -p "${dir}"
    printf '%s\n' "${dir}"
}

target_arch() {
    local arch
    arch="${CARGO_BUILD_TARGET:-$(uname -m)}"
    printf '%s\n' "${arch##*-}"
}

linux_deb_arch() {
    case "$(target_arch)" in
        x86_64|amd64) printf 'amd64\n' ;;
        aarch64|arm64) printf 'arm64\n' ;;
        armv7l) printf 'armhf\n' ;;
        *) target_arch ;;
    esac
}

macos_pkg_arch() {
    case "$(target_arch)" in
        x86_64|amd64) printf 'x86_64\n' ;;
        aarch64|arm64) printf 'arm64\n' ;;
        *) target_arch ;;
    esac
}
