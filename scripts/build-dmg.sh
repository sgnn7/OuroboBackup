#!/usr/bin/env bash
#
# Build a macOS installer DMG with drag-to-install layout.
#
# The DMG contains:
#   - OuroboBackup.app (GUI with daemon, CLI, and tray bundled inside)
#   - Applications symlink (for drag-to-install)
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
VERSION=$(grep '^version' "$PROJECT_ROOT/Cargo.toml" | head -1 | sed 's/.*"\(.*\)".*/\1/')
DMG_NAME="OuroboBackup-${VERSION}"
STAGING_DIR="$PROJECT_ROOT/target/dmg-staging"
DMG_OUTPUT="$PROJECT_ROOT/target/${DMG_NAME}.dmg"
APP_DIR="$STAGING_DIR/OuroboBackup.app"
APP_CONTENTS="$APP_DIR/Contents"
APP_MACOS="$APP_CONTENTS/MacOS"
APP_RESOURCES="$APP_CONTENTS/Resources"

echo "==> Building release binaries..."
cargo build --workspace --release

echo "==> Creating app bundle..."
rm -rf "$STAGING_DIR"
mkdir -p "$APP_MACOS" "$APP_RESOURCES"

# --- Info.plist ---
cat > "$APP_CONTENTS/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>OuroboBackup</string>
    <key>CFBundleDisplayName</key>
    <string>OuroboBackup</string>
    <key>CFBundleIdentifier</key>
    <string>com.ourobo.backup</string>
    <key>CFBundleVersion</key>
    <string>${VERSION}</string>
    <key>CFBundleShortVersionString</key>
    <string>${VERSION}</string>
    <key>CFBundleExecutable</key>
    <string>ourobo-launcher</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleIconFile</key>
    <string>AppIcon</string>
    <key>LSMinimumSystemVersion</key>
    <string>10.15</string>
    <key>LSApplicationCategoryType</key>
    <string>public.app-category.utilities</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>LSUIElement</key>
    <false/>
</dict>
</plist>
PLIST

# --- Launcher script ---
# Starts the daemon (if not running) and the GUI
cat > "$APP_MACOS/ourobo-launcher" <<'LAUNCHER'
#!/usr/bin/env bash
DIR="$(cd "$(dirname "$0")" && pwd)"

# Start daemon in background if not already running
if ! "$DIR/ourobo-cli" ping >/dev/null 2>&1; then
    "$DIR/ourobo-daemon" &
    sleep 1
fi

# Launch GUI
exec "$DIR/ourobo-gui"
LAUNCHER
chmod +x "$APP_MACOS/ourobo-launcher"

# --- Copy binaries into app bundle ---
BINARIES=(ourobo-daemon ourobo-cli ourobo-gui ourobo-tray)
for bin in "${BINARIES[@]}"; do
    if [ -f "$PROJECT_ROOT/target/release/$bin" ]; then
        cp "$PROJECT_ROOT/target/release/$bin" "$APP_MACOS/"
        echo "    Bundled $bin"
    else
        echo "    Warning: $bin not found, skipping"
    fi
done

# --- Copy resources ---
cp "$PROJECT_ROOT/config.example.toml" "$APP_RESOURCES/"

# --- Generate app icon (simple green circle) ---
# Uses built-in sips if no icon file exists
if [ ! -f "$APP_RESOURCES/AppIcon.icns" ]; then
    ICON_DIR=$(mktemp -d)
    ICONSET="$ICON_DIR/AppIcon.iconset"
    mkdir -p "$ICONSET"

    # Create a simple 512x512 green circle PNG using Python (available on macOS)
    python3 -c "
import struct, zlib

def create_png(size):
    def raw_data():
        cx, cy, r = size//2, size//2, size//2 - 20
        rows = []
        for y in range(size):
            row = b'\\x00'  # filter byte
            for x in range(size):
                dx, dy = x - cx, y - cy
                if dx*dx + dy*dy <= r*r:
                    row += b'\\x50\\xc8\\x50\\xff'  # green
                else:
                    row += b'\\x00\\x00\\x00\\x00'  # transparent
            rows.append(row)
        return b''.join(rows)

    raw = raw_data()
    def chunk(ctype, data):
        c = ctype + data
        return struct.pack('>I', len(data)) + c + struct.pack('>I', zlib.crc32(c) & 0xffffffff)

    ihdr = struct.pack('>IIBBBBB', size, size, 8, 6, 0, 0, 0)
    return (b'\\x89PNG\\r\\n\\x1a\\n' +
            chunk(b'IHDR', ihdr) +
            chunk(b'IDAT', zlib.compress(raw)) +
            chunk(b'IEND', b''))

for name, sz in [('icon_16x16.png',16),('icon_32x32.png',32),
                  ('icon_128x128.png',128),('icon_256x256.png',256),
                  ('icon_512x512.png',512)]:
    with open('$ICONSET/' + name, 'wb') as f:
        f.write(create_png(sz))
    " 2>/dev/null

    if [ -d "$ICONSET" ] && ls "$ICONSET"/*.png >/dev/null 2>&1; then
        iconutil -c icns -o "$APP_RESOURCES/AppIcon.icns" "$ICONSET" 2>/dev/null && \
            echo "    Generated AppIcon.icns" || \
            echo "    Warning: iconutil failed, app will use default icon"
    fi
    rm -rf "$ICON_DIR"
fi

# --- Applications symlink for drag-to-install ---
ln -sf /Applications "$STAGING_DIR/Applications"

# --- Create DMG ---
rm -f "$DMG_OUTPUT"

echo "==> Creating DMG..."
hdiutil create \
    -srcfolder "$STAGING_DIR" \
    -volname "$DMG_NAME" \
    -fs HFS+ \
    -format UDZO \
    -o "$DMG_OUTPUT"

# Clean up
rm -rf "$STAGING_DIR"

echo "==> Done: $DMG_OUTPUT"
echo "    Size: $(du -h "$DMG_OUTPUT" | cut -f1)"
echo ""
echo "    Install: Open the DMG and drag OuroboBackup to Applications."
