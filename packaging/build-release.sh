#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
case "${1:-$(uname -s)}" in
    Linux|linux)
        "${SCRIPT_DIR}/linux/deb/build-deb.sh"
        "${SCRIPT_DIR}/linux/build-portable.sh"
        ;;
    Darwin|macOS|macos)
        "${SCRIPT_DIR}/macos/build-pkg.sh"
        ;;
    Windows*|MINGW*|MSYS*|CYGWIN*|windows)
        pwsh -File "${SCRIPT_DIR}/windows/build-installer.ps1"
        ;;
    *)
        echo "Unsupported platform selector: ${1:-unknown}" >&2
        exit 1
        ;;
esac
