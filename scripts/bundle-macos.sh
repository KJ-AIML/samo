#!/usr/bin/env bash
# Builds Samo.app (release) into target/Samo.app. With --install, also puts it in /Applications and opens it.
#
#   scripts/bundle-macos.sh            # build target/Samo.app
#   scripts/bundle-macos.sh --install  # build, then install to /Applications
set -euo pipefail

cd "$(dirname "$0")/.."

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
APP="target/Samo.app"
DEST="/Applications/Samo.app"

echo "==> Building samo-gpui ${VERSION} (release)…"
cargo build --release -p samo-gpui

echo "==> Assembling ${APP}…"
mkdir -p "${APP}/Contents/MacOS" "${APP}/Contents/Resources"
cp target/release/samo-gpui "${APP}/Contents/MacOS/samo-gpui"
cp apps/samo-gpui/assets/Samo.icns "${APP}/Contents/Resources/Samo.icns"
cat > "${APP}/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>samo-gpui</string>
    <key>CFBundleIdentifier</key>
    <string>dev.talvyn.samo</string>
    <key>CFBundleName</key>
    <string>Samo</string>
    <key>CFBundleDisplayName</key>
    <string>Samo</string>
    <key>CFBundleIconFile</key>
    <string>Samo</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>${VERSION}</string>
    <key>CFBundleVersion</key>
    <string>${VERSION}</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>LSApplicationCategoryType</key>
    <string>public.app-category.utilities</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSHumanReadableCopyright</key>
    <string>MIT License</string>
</dict>
</plist>
PLIST

# An ad-hoc signature, so macOS treats the bundle as one signed app (no developer account needed).
codesign --force --deep --sign - "${APP}" >/dev/null 2>&1
echo "==> Built ${APP}"

if [ "${1:-}" = "--install" ]; then
  if pgrep -x samo-gpui >/dev/null; then
    echo "==> Quitting the running Samo…"
    pkill -x samo-gpui || true
    sleep 1
  fi
  echo "==> Installing to ${DEST}…"
  ditto "${APP}" "${DEST}"
  # Refresh Finder's and the Dock's idea of the icon.
  touch "${DEST}"
  open "${DEST}"
  echo "==> Samo is installed and running (look for the probe in the menu bar)."
fi
