#!/usr/bin/env bash
set -euo pipefail

if [ "$(id -u)" -ne 0 ] && [ -f "/usr/local/bin/zaluu" ] && command -v sudo >/dev/null 2>&1; then
    exec sudo bash "$0" "$@"
fi

rm -f /usr/local/bin/zaluu \
      /usr/local/share/applications/zaluu.desktop \
      /usr/local/share/icons/hicolor/256x256/apps/zaluu.png \
      "$HOME/.local/bin/zaluu" \
      "$HOME/.local/share/applications/zaluu.desktop" \
      "$HOME/.local/share/icons/hicolor/256x256/apps/zaluu.png"

echo "Đã gỡ cài đặt zaluu."
