#!/bin/bash
set -e
# Simple deb builder — creates a .deb package from the release build
VERSION="0.1.0"
ARCH=$(dpkg --print-architecture)
PKG_NAME="altermative_${VERSION}_${ARCH}"
BUILD_DIR="/tmp/${PKG_NAME}"

echo "Building release..."
cargo build --release

echo "Creating package structure..."
rm -rf "$BUILD_DIR"
mkdir -p "$BUILD_DIR/DEBIAN"
mkdir -p "$BUILD_DIR/usr/bin"
mkdir -p "$BUILD_DIR/usr/share/applications"
mkdir -p "$BUILD_DIR/usr/share/icons/hicolor/256x256/apps"

cp packaging/linux/deb/control "$BUILD_DIR/DEBIAN/"
sed -i "s/Architecture: .*/Architecture: $ARCH/" "$BUILD_DIR/DEBIAN/control"
cp target/release/alterm "$BUILD_DIR/usr/bin/"
cp packaging/linux/altermative.desktop "$BUILD_DIR/usr/share/applications/"
# Icon would go here if we had one

echo "Building .deb..."
dpkg-deb --build "$BUILD_DIR"
mv "/tmp/${PKG_NAME}.deb" .

echo "Package created: ${PKG_NAME}.deb"
rm -rf "$BUILD_DIR"
