#!/bin/bash
set -e

echo "Building Altermative..."
cargo build --release

echo "Installing binary..."
install -Dm755 target/release/alterm "$HOME/.cargo/bin/alterm"

echo "Installing desktop entry..."
install -Dm644 packaging/linux/altermative.desktop "$HOME/.local/share/applications/altermative.desktop"

# Install icon if it exists
if [ -f assets/icons/altermative.png ]; then
    install -Dm644 assets/icons/altermative.png "$HOME/.local/share/icons/hicolor/256x256/apps/altermative.png"
fi

echo "Creating default config..."
mkdir -p "$HOME/.config/altermative"
if [ ! -f "$HOME/.config/altermative/config.toml" ]; then
    cp config/default.toml "$HOME/.config/altermative/config.toml"
    echo "Default config created at ~/.config/altermative/config.toml"
fi

echo ""
echo "Altermative installed successfully!"
echo "  Binary: ~/.cargo/bin/alterm"
echo "  Desktop: ~/.local/share/applications/altermative.desktop"
echo "  Config: ~/.config/altermative/config.toml"
echo ""
echo "You may need to log out and back in for the desktop entry to appear."
