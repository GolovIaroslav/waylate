#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PREFIX="${PREFIX:-$HOME/.local}"

if [[ ! -f "$ROOT/bin/waylate" ]]; then
  echo "Error: this script must be run from inside the release archive, not from the source repo."
  echo ""
  echo "Download the AppImage from https://github.com/GolovIaroslav/waylate/releases/latest"
  echo "  chmod +x Waylate-*-x86_64.AppImage"
  echo "  ./Waylate-*-x86_64.AppImage"
  exit 1
fi

install -Dm755 "$ROOT/bin/waylate" "$PREFIX/bin/waylate"
install -Dm644 "$ROOT/share/applications/dev.jar.waylate.desktop" \
  "$PREFIX/share/applications/dev.jar.waylate.desktop"
install -Dm644 "$ROOT/share/icons/hicolor/128x128/apps/dev.jar.waylate.png" \
  "$PREFIX/share/icons/hicolor/128x128/apps/dev.jar.waylate.png"
install -Dm644 "$ROOT/share/licenses/waylate/LICENSE" \
  "$PREFIX/share/licenses/waylate/LICENSE"
install -Dm644 "$ROOT/share/doc/waylate/README.md" \
  "$PREFIX/share/doc/waylate/README.md"

if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$PREFIX/share/applications" >/dev/null 2>&1 || true
fi

cat <<MSG
Waylate installed to $PREFIX

Make sure this directory is in PATH:
  $PREFIX/bin

Run:
  waylate

For KDE Plasma shortcut use:
  waylate translate-selection
MSG
