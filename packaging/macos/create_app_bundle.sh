#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
APP_NAME="Zalo Rust"
APP_BUNDLE="$PROJECT_DIR/${APP_NAME}.app"
APPLICATIONS_BUNDLE="/Applications/${APP_NAME}.app"

echo "=== Tạo macOS App Bundle: ${APP_NAME}.app ==="

# 1. Chuẩn bị cấu trúc bundle
rm -rf "$APP_BUNDLE"
mkdir -p "$APP_BUNDLE/Contents/MacOS"
mkdir -p "$APP_BUNDLE/Contents/Resources"

# 2. Copy binary (ưu tiên release, fallback sang debug)
if [[ -f "$PROJECT_DIR/target/release/ctdozlite" ]]; then
    echo "Sử dụng binary Release tối ưu..."
    cp "$PROJECT_DIR/target/release/ctdozlite" "$APP_BUNDLE/Contents/MacOS/ctdozlite"
    strip "$APP_BUNDLE/Contents/MacOS/ctdozlite" || true
elif [[ -f "$PROJECT_DIR/target/debug/ctdozlite" ]]; then
    echo "Sử dụng binary Debug..."
    cp "$PROJECT_DIR/target/debug/ctdozlite" "$APP_BUNDLE/Contents/MacOS/ctdozlite"
else
    echo "Lỗi: Không tìm thấy binary ctdozlite!" >&2
    exit 1
fi
chmod +x "$APP_BUNDLE/Contents/MacOS/ctdozlite"

# 2b. Copy assets/WASM cho Sync v2 & zproto
if [[ -d "$PROJECT_DIR/scripts/assets" ]]; then
    mkdir -p "$APP_BUNDLE/Contents/Resources/scripts/assets"
    cp -R "$PROJECT_DIR/scripts/assets/"* "$APP_BUNDLE/Contents/Resources/scripts/assets/" 2>/dev/null || true
fi
ICONSET_TMP="$(mktemp -d)/AppIcon.iconset"
mkdir -p "$ICONSET_TMP"
SRC_ICON="$PROJECT_DIR/src/assets/app_icon.png"

for sz in 16 32 64 128 256; do
    sips -s format png -z "$sz" "$sz" "$SRC_ICON" --out "$ICONSET_TMP/icon_${sz}x${sz}.png" >/dev/null 2>&1
    sz2=$((sz * 2))
    if [[ $sz2 -le 512 ]]; then
        sips -s format png -z "$sz2" "$sz2" "$SRC_ICON" --out "$ICONSET_TMP/icon_${sz}x${sz}@2x.png" >/dev/null 2>&1
    fi
done
iconutil -c icns "$ICONSET_TMP" -o "$APP_BUNDLE/Contents/Resources/AppIcon.icns"
rm -rf "$(dirname "$ICONSET_TMP")"

# 4. Tạo Info.plist
cat << 'EOF' > "$APP_BUNDLE/Contents/Info.plist"
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>vi</string>
    <key>CFBundleDisplayName</key>
    <string>Zalo Rust</string>
    <key>CFBundleExecutable</key>
    <string>ctdozlite</string>
    <key>CFBundleIconFile</key>
    <string>AppIcon</string>
    <key>CFBundleIconName</key>
    <string>AppIcon</string>
    <key>CFBundleIdentifier</key>
    <string>com.ctdoteam1110.ctdozlite</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>Zalo Rust</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.0</string>
    <key>CFBundleVersion</key>
    <string>1</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSMicrophoneUsageDescription</key>
    <string>Zalo Rust cần quyền Microphone cho cuộc gọi Zalo.</string>
    <key>NSCameraUsageDescription</key>
    <string>Zalo Rust cần quyền Camera cho cuộc gọi video Zalo.</string>
    <key>NSPrincipalClass</key>
    <string>NSApplication</string>
</dict>
</plist>
EOF

printf 'APPL????' > "$APP_BUNDLE/Contents/PkgInfo"

# 5. Ad-hoc codesign cho macOS
xattr -cr "$APP_BUNDLE" || true
codesign --force --deep --sign - "$APP_BUNDLE"

# 6. Đồng bộ dữ liệu phiên hiện tại vào ~/Library/Application Support/ctdozlite
# Giúp người dùng khi click app từ Dock sẽ vào thẳng tài khoản đã đăng nhập mà không cần quét lại QR!
DATA_DIR="$HOME/Library/Application Support/ctdozlite"
mkdir -p "$DATA_DIR"
if [[ -f "$PROJECT_DIR/session.json" ]]; then
    cp "$PROJECT_DIR/session.json" "$DATA_DIR/session.json"
fi
if [[ -f "$PROJECT_DIR/threads_cache.json" ]]; then
    cp "$PROJECT_DIR/threads_cache.json" "$DATA_DIR/threads_cache.json"
fi
if [[ -f "$PROJECT_DIR/zalo_history.db" ]]; then
    cp "$PROJECT_DIR/zalo_history.db" "$DATA_DIR/zalo_history.db"
fi
if [[ -f "$PROJECT_DIR/app_settings.json" ]]; then
    cp "$PROJECT_DIR/app_settings.json" "$DATA_DIR/app_settings.json"
fi
if [[ -d "$PROJECT_DIR/stickers_cache" ]]; then
    cp -R "$PROJECT_DIR/stickers_cache" "$DATA_DIR/"
fi

# 7. Cài đặt vào /Applications
echo "Cài đặt vào /Applications..."
rm -rf "$APPLICATIONS_BUNDLE"
cp -R "$APP_BUNDLE" "$APPLICATIONS_BUNDLE"

echo "=== Hoàn thành tạo $APPLICATIONS_BUNDLE và $APP_BUNDLE! ==="
