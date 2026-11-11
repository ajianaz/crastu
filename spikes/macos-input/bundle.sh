#!/bin/sh
# Builds the spike and wraps it in a windowless agent app, so macOS privacy
# permissions (Accessibility, Input Monitoring) belong to the spike itself rather
# than to the terminal that launched it. Ad-hoc signed with a fixed identifier.
#
#   ./bundle.sh
#   open -n target/CrastuInputSpike.app --args check --prompt
#   open -n target/CrastuInputSpike.app --args run --key rightoption --sink paste-promise
#   tail -f ~/Library/Logs/crastu-spike-input.log
set -eu
cd "$(dirname "$0")"
cargo build --release
app=target/CrastuInputSpike.app
rm -rf "$app"
mkdir -p "$app/Contents/MacOS"
cp target/release/spike-macos-input "$app/Contents/MacOS/spike-macos-input"
cat > "$app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key><string>dev.crastu.spike.input</string>
  <key>CFBundleName</key><string>Crastu Input Spike</string>
  <key>CFBundleExecutable</key><string>spike-macos-input</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.0.0</string>
  <key>CFBundleVersion</key><string>0</string>
  <key>LSMinimumSystemVersion</key><string>14.0</string>
  <key>LSUIElement</key><true/>
</dict>
</plist>
PLIST
codesign --force --sign - --identifier dev.crastu.spike.input "$app"
codesign --verify --verbose "$app"
echo "built $app"
