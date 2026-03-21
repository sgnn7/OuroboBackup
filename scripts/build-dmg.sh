#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
VERSION=$(grep '^version' "$PROJECT_ROOT/Cargo.toml" | head -1 | sed 's/.*"\(.*\)".*/\1/')
DMG_NAME="OuroboBackup-${VERSION}"
STAGING_DIR="$PROJECT_ROOT/target/dmg-staging"
DMG_OUTPUT="$PROJECT_ROOT/target/${DMG_NAME}.dmg"

echo "==> Building release binaries..."
cargo build --workspace --release

echo "==> Creating staging directory..."
rm -rf "$STAGING_DIR"
mkdir -p "$STAGING_DIR"

# Copy binaries
BINARIES=(ourobo-daemon ourobo-cli ourobo-gui ourobo-tray)
for bin in "${BINARIES[@]}"; do
    if [ -f "$PROJECT_ROOT/target/release/$bin" ]; then
        cp "$PROJECT_ROOT/target/release/$bin" "$STAGING_DIR/"
        echo "    Copied $bin"
    else
        echo "    Warning: $bin not found, skipping"
    fi
done

# Copy config example
cp "$PROJECT_ROOT/config.example.toml" "$STAGING_DIR/"
cp "$PROJECT_ROOT/README.md" "$STAGING_DIR/"

# Try cargo-bundle for .app bundles if available
if command -v cargo-bundle &>/dev/null; then
    echo "==> Creating .app bundles with cargo-bundle..."
    (cd "$PROJECT_ROOT" && cargo bundle --release -p ourobo-gui 2>/dev/null) && \
        cp -r "$PROJECT_ROOT/target/release/bundle/osx/OuroboBackup.app" "$STAGING_DIR/" && \
        echo "    Created OuroboBackup.app" || \
        echo "    Skipped OuroboBackup.app (cargo-bundle failed)"

    (cd "$PROJECT_ROOT" && cargo bundle --release -p ourobo-tray 2>/dev/null) && \
        cp -r "$PROJECT_ROOT/target/release/bundle/osx/OuroboBackup Tray.app" "$STAGING_DIR/" && \
        echo "    Created OuroboBackup Tray.app" || \
        echo "    Skipped OuroboBackup Tray.app (cargo-bundle failed)"
else
    echo "==> cargo-bundle not found, skipping .app bundles"
    echo "    Install with: cargo install cargo-bundle"
fi

# Remove any existing DMG
rm -f "$DMG_OUTPUT"

echo "==> Creating DMG..."
hdiutil create \
    -srcfolder "$STAGING_DIR" \
    -volname "$DMG_NAME" \
    -fs HFS+ \
    -format UDZO \
    -o "$DMG_OUTPUT"

# Clean up staging
rm -rf "$STAGING_DIR"

echo "==> Done: $DMG_OUTPUT"
echo "    Size: $(du -h "$DMG_OUTPUT" | cut -f1)"
