#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common/metadata.sh
source "${SCRIPT_DIR}/common/metadata.sh"

echo "Uninstalling Alterm..."

rm -f "$HOME/.cargo/bin/${BINARY_NAME}"
rm -f "$HOME/.local/share/applications/alterm.desktop"
rm -f "$HOME/.config/alterm/hooks.lua.example"

echo "Alterm uninstalled."
echo "Config files at ~/.config/alterm/ were NOT removed."
