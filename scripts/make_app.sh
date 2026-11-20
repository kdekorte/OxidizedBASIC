#!/bin/bash
# Build OxidizedBASIC.app: a macOS application bundle for the IDE.
#
#   scripts/make_app.sh            build the release binary and the bundle
#   scripts/make_app.sh --open     build, then open the app
#
# The bundle is written to dist/OxidizedBASIC.app. It holds the release
# `oxbasic` binary (started with `ide`), an Info.plist, and the app icon.
# The icon comes from packaging/icons, which is the Icon Package's contents:
# the .iconset is the source, and the .icns is rebuilt from it with iconutil
# whenever the iconset is newer, so the bundle always has every size.
#
# The app is ad-hoc signed, which is enough to run it on this Mac. A copy for
# another Mac needs a Developer ID signature and notarisation (not done here).
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT="$(pwd)"
APP_NAME="OxidizedBASIC"
BUNDLE="dist/${APP_NAME}.app"
ICONSET="packaging/icons/${APP_NAME}.iconset"
ICNS="packaging/icons/${APP_NAME}.icns"

# the version comes from the workspace, so the bundle always matches it
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
BUILD="$(git rev-parse --short HEAD 2>/dev/null || echo dev)"

echo "==> release build"
cargo build --release -p oxbasic_ide

echo "==> icon"
if [ ! -f "$ICNS" ] || [ "$ICONSET" -nt "$ICNS" ]; then
    iconutil -c icns "$ICONSET" -o "$ICNS"
    echo "    rebuilt $ICNS from $ICONSET"
else
    echo "    up to date: $ICNS"
fi

echo "==> bundle"
rm -rf "$BUNDLE"
mkdir -p "$BUNDLE/Contents/MacOS" "$BUNDLE/Contents/Resources"
cp target/release/oxbasic "$BUNDLE/Contents/MacOS/${APP_NAME}"
cp "$ICNS" "$BUNDLE/Contents/Resources/${APP_NAME}.icns"

# the double-click passes no arguments; `ide` with none opens a new project
cat > "$BUNDLE/Contents/MacOS/${APP_NAME}-launch" <<'LAUNCHER'
#!/bin/bash
exec "$(dirname "$0")/OxidizedBASIC" ide
LAUNCHER
chmod +x "$BUNDLE/Contents/MacOS/${APP_NAME}-launch"

cat > "$BUNDLE/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>${APP_NAME}</string>
    <key>CFBundleDisplayName</key>
    <string>${APP_NAME}</string>
    <key>CFBundleIdentifier</key>
    <string>com.oxidizedbasic.ide</string>
    <key>CFBundleVersion</key>
    <string>${BUILD}</string>
    <key>CFBundleShortVersionString</key>
    <string>${VERSION}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleExecutable</key>
    <string>${APP_NAME}-launch</string>
    <key>CFBundleIconFile</key>
    <string>${APP_NAME}</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSPrincipalClass</key>
    <string>NSApplication</string>
    <key>CFBundleDocumentTypes</key>
    <array>
        <dict>
            <key>CFBundleTypeName</key>
            <string>OxidizedBASIC Project</string>
            <key>CFBundleTypeRole</key>
            <string>Editor</string>
            <key>LSItemContentTypes</key>
            <array>
                <string>com.oxidizedbasic.project</string>
            </array>
        </dict>
    </array>
</dict>
</plist>
PLIST
plutil -lint "$BUNDLE/Contents/Info.plist" >/dev/null

echo "==> sign (ad-hoc)"
codesign --force --deep --sign - "$BUNDLE" >/dev/null
codesign --verify --deep --strict "$BUNDLE"

echo "==> done: $ROOT/$BUNDLE (version ${VERSION}, build ${BUILD})"
if [ "${1:-}" = "--open" ]; then
    open "$BUNDLE"
fi
