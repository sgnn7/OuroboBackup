#!/usr/bin/env bash
#
# Build a macOS installer DMG with drag-to-install UX.
#
# Opens as a styled Finder window with the app on the left,
# an arrow, and an Applications shortcut on the right.
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
VERSION=$(grep '^version' "$PROJECT_ROOT/Cargo.toml" | head -1 | sed 's/.*"\(.*\)".*/\1/')
DMG_NAME="OuroboBackup-${VERSION}"
STAGING_DIR="$PROJECT_ROOT/target/dmg-staging"
DMG_RW="$PROJECT_ROOT/target/${DMG_NAME}-rw.dmg"
DMG_OUTPUT="$PROJECT_ROOT/target/${DMG_NAME}.dmg"
VOLUME_NAME="$DMG_NAME"
APP_DIR="$STAGING_DIR/OuroboBackup.app"
APP_CONTENTS="$APP_DIR/Contents"
APP_MACOS="$APP_CONTENTS/MacOS"
APP_RESOURCES="$APP_CONTENTS/Resources"

WIN_W=540
WIN_H=380
ICON_SIZE=128

# Pre-compute icon positions for AppleScript
APP_X=$(( WIN_W / 4 ))
APP_Y=$(( WIN_H / 2 - 20 ))
APPS_X=$(( WIN_W * 3 / 4 ))

# Track mount state for cleanup
DMG_DEV=""

cleanup() {
    if [ -n "$DMG_DEV" ]; then
        echo "==> Cleanup: detaching $DMG_DEV..."
        hdiutil detach "$DMG_DEV" -force 2>&1 || echo "    Warning: failed to detach $DMG_DEV during cleanup"
    fi
    rm -f "$DMG_RW"
    rm -rf "$STAGING_DIR"
}
trap cleanup EXIT

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
    <string>org.sgnn7.ourobobackup</string>
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
cat > "$APP_MACOS/ourobo-launcher" <<'LAUNCHER'
#!/usr/bin/env bash
DIR="$(cd "$(dirname "$0")" && pwd)"
LOG_DIR="$HOME/.ourobo"
mkdir -p "$LOG_DIR"

# Start daemon in background if not already running
if ! "$DIR/ourobo-cli" ping >/dev/null 2>&1; then
    "$DIR/ourobo-daemon" >> "$LOG_DIR/daemon.log" 2>&1 &
    DAEMON_PID=$!

    # Wait for daemon to be ready (up to 5 seconds)
    for _ in $(seq 1 50); do
        if "$DIR/ourobo-cli" ping >/dev/null 2>&1; then
            break
        fi
        # Fast-fail if daemon process already exited
        if ! kill -0 "$DAEMON_PID" 2>/dev/null; then
            break
        fi
        sleep 0.1
    done

    if ! "$DIR/ourobo-cli" ping >/dev/null 2>&1; then
        osascript -e 'display alert "OuroboBackup" message "Daemon failed to start. Check ~/.ourobo/daemon.log for details." as warning' 2>/dev/null \
            || echo "WARNING: Daemon failed to start. Check ~/.ourobo/daemon.log" >&2
    fi
fi

# Start tray icon in background (if present)
if [ -x "$DIR/ourobo-tray" ]; then
    "$DIR/ourobo-tray" >> "$LOG_DIR/tray.log" 2>&1 &
elif [ -f "$DIR/ourobo-tray" ]; then
    echo "WARNING: ourobo-tray exists but is not executable" >> "$LOG_DIR/tray.log"
fi

# Launch GUI
exec "$DIR/ourobo-gui"
LAUNCHER
chmod +x "$APP_MACOS/ourobo-launcher"

# --- Copy binaries ---
BINARIES=(ourobo-daemon ourobo-cli ourobo-gui ourobo-tray)
for bin in "${BINARIES[@]}"; do
    if [ ! -f "$PROJECT_ROOT/target/release/$bin" ]; then
        echo "ERROR: Required binary $bin not found in target/release/"
        exit 1
    fi
    cp "$PROJECT_ROOT/target/release/$bin" "$APP_MACOS/"
    echo "    Bundled $bin"
done

# Verify launcher is present
if [ ! -f "$APP_MACOS/ourobo-launcher" ]; then
    echo "ERROR: Launcher script is missing from app bundle"
    exit 1
fi

# --- Copy resources ---
cp "$PROJECT_ROOT/config.example.toml" "$APP_RESOURCES/"

# --- Generate app icon from project source icon ---
ICON_SOURCE="$PROJECT_ROOT/assets/icon_source.png"
if [ ! -f "$APP_RESOURCES/AppIcon.icns" ] && [ -f "$ICON_SOURCE" ]; then
    ICON_DIR=$(mktemp -d)
    ICONSET="$ICON_DIR/AppIcon.iconset"
    mkdir -p "$ICONSET"

    for sz in 16 32 128 256 512; do
        sips -z $sz $sz "$ICON_SOURCE" --out "$ICONSET/icon_${sz}x${sz}.png" >/dev/null 2>&1
        sz2=$(( sz * 2 ))
        sips -z $sz2 $sz2 "$ICON_SOURCE" --out "$ICONSET/icon_${sz}x${sz}@2x.png" >/dev/null 2>&1
    done

    if iconutil -c icns -o "$APP_RESOURCES/AppIcon.icns" "$ICONSET"; then
        echo "    Generated AppIcon.icns from assets/icon_source.png"
    else
        echo "    Warning: iconutil failed, app will use default icon"
    fi
    rm -rf "$ICON_DIR"
elif [ ! -f "$ICON_SOURCE" ]; then
    echo "    Warning: assets/icon_source.png not found, app will use default icon"
fi

# --- Applications symlink ---
ln -sf /Applications "$STAGING_DIR/Applications"

# --- Generate background image with arrow ---
echo "==> Generating installer background..."
BG_DIR="$STAGING_DIR/.background"
mkdir -p "$BG_DIR"

if ! python3 -c "
import struct, zlib

W, H = ${WIN_W} * 2, ${WIN_H} * 2  # retina

def create_png():
    rows = []
    ax1, ax2 = int(W * 0.33), int(W * 0.67)
    ay = H // 2 + 40
    arrow_w = 6
    head_len = 60
    head_w = 30

    for y in range(H):
        row = b'\\x00'
        for x in range(W):
            r, g, b, a = 0xf5, 0xf5, 0xf5, 0xff

            # Arrow shaft
            if ax1 <= x <= ax2 - head_len and abs(y - ay) <= arrow_w:
                r, g, b = 0x88, 0x88, 0x88

            # Arrow head
            tip_x = ax2
            base_x = ax2 - head_len
            if base_x <= x <= tip_x:
                progress = (x - base_x) / head_len
                half_h = head_w * (1.0 - progress)
                if abs(y - ay) <= half_h:
                    r, g, b = 0x88, 0x88, 0x88

            row += bytes([r, g, b, a])
        rows.append(row)

    raw = b''.join(rows)
    def chunk(ctype, data):
        c = ctype + data
        return struct.pack('>I', len(data)) + c + struct.pack('>I', zlib.crc32(c) & 0xffffffff)

    ihdr = struct.pack('>IIBBBBB', W, H, 8, 6, 0, 0, 0)
    return (b'\\x89PNG\\r\\n\\x1a\\n' +
            chunk(b'IHDR', ihdr) +
            chunk(b'IDAT', zlib.compress(raw, 1)) +
            chunk(b'IEND', b''))

with open('${BG_DIR}/background.png', 'wb') as f:
    f.write(create_png())
"; then
    echo "    Warning: background generation failed (see error above), continuing without background"
else
    echo "    Generated background.png"
fi

# --- Volume icon ---
if [ -f "$APP_RESOURCES/AppIcon.icns" ]; then
    cp "$APP_RESOURCES/AppIcon.icns" "$STAGING_DIR/.VolumeIcon.icns"
fi

# --- Calculate DMG size ---
STAGING_SIZE_MB=$(du -sm "$STAGING_DIR" | awk '{print $1}')
DMG_SIZE_MB=$(( STAGING_SIZE_MB + 10 ))  # headroom for filesystem overhead
echo "==> Staging size: ${STAGING_SIZE_MB}MB, DMG size: ${DMG_SIZE_MB}MB"

# --- Create DMG ---
rm -f "$DMG_OUTPUT" "$DMG_RW"

echo "==> Creating writable DMG..."
hdiutil create \
    -srcfolder "$STAGING_DIR" \
    -volname "$VOLUME_NAME" \
    -fs HFS+ \
    -format UDRW \
    -size "${DMG_SIZE_MB}m" \
    -ov \
    -o "${DMG_RW%.dmg}"

# Detach any stale mount with this volume name
if mount | grep -q "/Volumes/$VOLUME_NAME"; then
    hdiutil detach "/Volumes/$VOLUME_NAME" || {
        echo "ERROR: Cannot detach existing mount at /Volumes/$VOLUME_NAME"
        exit 1
    }
fi

# Mount writable DMG, capture device for reliable detach
ATTACH_OUT=$(hdiutil attach -readwrite -noverify -noautoopen "$DMG_RW")
DMG_DEV=$(echo "$ATTACH_OUT" | head -1 | awk '{print $1}')
MOUNT_DIR=$(echo "$ATTACH_OUT" | grep Apple_HFS | sed 's|.*Apple_HFS[[:space:]]*||')

if [ -z "$DMG_DEV" ] || [ -z "$MOUNT_DIR" ]; then
    echo "ERROR: Failed to parse hdiutil attach output:"
    echo "$ATTACH_OUT"
    exit 1
fi
sleep 1

# Set custom volume icon
if command -v SetFile >/dev/null 2>&1; then
    if [ -f "$MOUNT_DIR/.VolumeIcon.icns" ]; then
        SetFile -a C "$MOUNT_DIR"
    fi
else
    echo "    Note: SetFile not found (install Xcode CLT); volume icon may not display"
fi

# Try AppleScript for window styling (works if Finder automation is allowed)
echo "==> Styling DMG window..."
osascript <<APPLESCRIPT 2>&1 || echo "    Note: Finder styling skipped (no automation permission). DMG will use default icon view."
tell application "Finder"
    tell disk "${VOLUME_NAME}"
        open
        set current view of container window to icon view
        set toolbar visible of container window to false
        set statusbar visible of container window to false
        set bounds of container window to {100, 100, $((100 + WIN_W)), $((100 + WIN_H))}

        set theViewOptions to icon view options of container window
        set arrangement of theViewOptions to not arranged
        set icon size of theViewOptions to ${ICON_SIZE}
        set background picture of theViewOptions to file ".background:background.png"

        set position of item "OuroboBackup.app" of container window to {${APP_X}, ${APP_Y}}
        set position of item "Applications" of container window to {${APPS_X}, ${APP_Y}}

        close
        open
        update without registering applications
        delay 1
        close
    end tell
end tell
APPLESCRIPT

sync
hdiutil detach "$DMG_DEV" || {
    echo "    Warning: clean detach failed, retrying with -force..."
    sleep 2
    hdiutil detach "$DMG_DEV" -force
}
DMG_DEV=""  # cleared so trap doesn't double-detach

# Convert to compressed read-only
echo "==> Compressing DMG..."
hdiutil convert "$DMG_RW" \
    -format UDZO \
    -imagekey zlib-level=9 \
    -ov \
    -o "${DMG_OUTPUT%.dmg}"

rm -f "$DMG_RW"
rm -rf "$STAGING_DIR"

# Disable trap since we cleaned up successfully
trap - EXIT

echo "==> Done: $DMG_OUTPUT"
echo "    Size: $(du -h "$DMG_OUTPUT" | cut -f1)"
echo ""
echo "    Install: Open the DMG and drag OuroboBackup to Applications."
