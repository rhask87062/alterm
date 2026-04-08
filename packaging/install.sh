#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common/metadata.sh
source "${SCRIPT_DIR}/common/metadata.sh"

cd "${REPO_ROOT}"

echo "Building Alterm..."
cargo build --release --package "${PACKAGE_NAME}"

echo "Installing binary..."
install -Dm755 "target/release/${BINARY_NAME}" "$HOME/.cargo/bin/${BINARY_NAME}"

echo "Installing desktop entry..."
install -Dm644 "packaging/linux/alterm.desktop" "$HOME/.local/share/applications/alterm.desktop"
if [ -f "${LINUX_ICON_PATH}" ]; then
    echo "Installing app icon..."
    install -Dm644 "${LINUX_ICON_PATH}" "$HOME/.local/share/icons/hicolor/256x256/apps/alterm.png"
fi

echo "Creating default config..."
mkdir -p "$HOME/.config/alterm"
if [ ! -f "$HOME/.config/alterm/config.toml" ]; then
    cp "config/default.toml" "$HOME/.config/alterm/config.toml"
    echo "Default config created at ~/.config/alterm/config.toml"
fi
install -Dm644 "config/hooks.lua.example" "$HOME/.config/alterm/hooks.lua.example"

echo ""
echo "Alterm installed successfully!"
echo "  Binary: ~/.cargo/bin/${BINARY_NAME}"
echo "  Desktop: ~/.local/share/applications/alterm.desktop"
echo "  Config: ~/.config/alterm/config.toml"
echo "  Hooks example: ~/.config/alterm/hooks.lua.example"
echo ""
echo "You may need to log out and back in for the desktop entry to appear."
