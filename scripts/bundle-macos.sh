#!/bin/sh
# Build dist/drift.app for this Mac: the release binary, the icon, and an identity of
# its own (io.github.domknez.drift), which macOS needs to show drift's notifications.
# The app is signed ad hoc; macOS asks once before opening it.
#
# DRIFT_BINARY=<path> bundles that binary instead of building one (the release
# workflow's universal build).
set -e
cd "$(dirname "$0")/.."

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
app=dist/drift.app
export MACOSX_DEPLOYMENT_TARGET=11.0

binary=${DRIFT_BINARY:-}
if [ -z "$binary" ]; then
  cargo build --release --locked
  binary=target/release/drift
fi
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$binary" "$app/Contents/MacOS/drift"
cp assets/icon/drift.icns "$app/Contents/Resources/drift.icns"

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>drift</string>
  <key>CFBundleDisplayName</key><string>drift</string>
  <key>CFBundleIdentifier</key><string>io.github.domknez.drift</string>
  <key>CFBundleExecutable</key><string>drift</string>
  <key>CFBundleIconFile</key><string>drift</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>$MACOSX_DEPLOYMENT_TARGET</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.business</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSUserNotificationAlertStyle</key><string>banner</string>
  <key>NSHumanReadableCopyright</key><string>MIT License. Free and open source.</string>
</dict>
</plist>
PLIST

codesign --force --sign - "$app"
echo "$app"
