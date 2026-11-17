#!/bin/sh
# Builds the UIKit-only Caltrain push reference into this worktree's target directory.
set -eu
cd "$(dirname "$0")/../.."
app="$PWD/target/bareref/BareUIKit.app"
mkdir -p "$app"
cp bench/bareref/stations.json bench/bareref/home.json "$app/"
cp apps/caltrain/assets/caltrain.png "$app/"
cat > "$app/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>BareUIKit</string>
<key>CFBundleIdentifier</key><string>dev.exact.bareref</string>
<key>CFBundleName</key><string>Bare UIKit</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>1</string>
<key>CFBundleShortVersionString</key><string>1.0</string>
<key>MinimumOSVersion</key><string>17.0</string>
<key>UIDeviceFamily</key><array><integer>1</integer></array>
<key>UILaunchScreen</key><dict/>
<key>UIApplicationSceneManifest</key><dict><key>UIApplicationSupportsMultipleScenes</key><false/><key>UISceneConfigurations</key><dict><key>UIWindowSceneSessionRoleApplication</key><array><dict><key>UISceneConfigurationName</key><string>Default</string><key>UISceneDelegateClassName</key><string>BareUIKit.Scene</string></dict></array></dict></dict>
</dict></plist>
PLIST
xcrun -sdk iphonesimulator swiftc -parse-as-library -O -g \
  -target arm64-apple-ios17.0-simulator bench/bareref/App.swift bench/bareref/PushProbe.swift \
  -o "$app/BareUIKit"
codesign -s - -f "$app"
