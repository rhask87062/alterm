#!/bin/bash
set -e

echo "Uninstalling Altermative..."

rm -f "$HOME/.cargo/bin/alterm"
rm -f "$HOME/.local/share/applications/altermative.desktop"
rm -f "$HOME/.local/share/icons/hicolor/256x256/apps/altermative.png"

echo "Altermative uninstalled."
echo "Config files at ~/.config/altermative/ were NOT removed."
