#!/bin/bash
# Builds "dist/MediaFiles Ferry.app" as a universal binary (Apple Silicon + Intel)
# and "dist/MediaFiles-Ferry-<version>.zip". Usage: scripts/build-mac-app.sh
# Requires: rustup target add x86_64-apple-darwin
set -euo pipefail
cd "$(dirname "$0")/.."
NAME="MediaFiles Ferry"
RELEASE=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)   # e.g. 1.0.0-beta.1
VERSION=${RELEASE%%-*}                                                  # numbers only for macOS
BUILD=1                                                                 # bump with every release
APP="dist/$NAME.app"

cargo build --release --bin ferry-app --target aarch64-apple-darwin
cargo build --release --bin ferry-app --target x86_64-apple-darwin
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create target/aarch64-apple-darwin/release/ferry-app target/x86_64-apple-darwin/release/ferry-app \
  -output "$APP/Contents/MacOS/$NAME"

# every size designed on Apple's grid (assets/AppIcon.iconset), not scaled down
iconutil -c icns assets/AppIcon.iconset -o "$APP/Contents/Resources/AppIcon.icns"
# license texts ship with the app (GPL for Ferry, OFL for the embedded font)
cp LICENSE "$APP/Contents/Resources/LICENSE.txt"
cp ui/fonts/OFL.txt "$APP/Contents/Resources/OFL.txt"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>$NAME</string>
  <key>CFBundleDisplayName</key><string>$NAME</string>
  <key>CFBundleExecutable</key><string>$NAME</string>
  <key>CFBundleIdentifier</key><string>io.github.moritz-poth.mediafiles-ferry</string>
  <key>CFBundleVersion</key><string>$BUILD</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>MediaFilesFerryRelease</key><string>$RELEASE</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSHumanReadableCopyright</key><string>© 2026 Moritz Poth. GPL-3.0-or-later.</string>
</dict>
</plist>
PLIST
# ad-hoc signed (no Apple Developer account): macOS needs "Open Anyway" once, see README
codesign --force --deep --sign - "$APP"
ZIP="dist/MediaFiles-Ferry-$RELEASE.zip"
rm -f "$ZIP"
ditto -c -k --keepParent "$APP" "$ZIP"
echo "Done: $APP, $ZIP"
