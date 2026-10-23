#!/bin/sh
set -eu
project_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repository_dir=$(dirname "$project_dir")
package_dir="$repository_dir/outputs/rust-build"
app_dir="$package_dir/星笺.app"

cargo build --manifest-path "$project_dir/Cargo.toml" --release --locked
mkdir -p "$app_dir/Contents/MacOS" "$app_dir/Contents/Resources"
cp "$project_dir/target/release/xingjian" "$app_dir/Contents/MacOS/xingjian"
cp "$repository_dir/LICENSE" "$app_dir/Contents/Resources/LICENSE"
cat > "$app_dir/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>星笺</string>
<key>CFBundleDisplayName</key><string>星笺</string>
<key>CFBundleIdentifier</key><string>org.handwritinggenerator.xingjian.rust</string>
<key>CFBundleExecutable</key><string>xingjian</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>CFBundleVersion</key><string>1</string>
<key>CFBundleIconFile</key><string>AppIcon</string>
<key>LSMinimumSystemVersion</key><string>11.0</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST

icon_dir=$(mktemp -d "$package_dir/icon.XXXXXX")
trap 'rm -rf "$icon_dir"' EXIT
mkdir "$icon_dir/AppIcon.iconset"
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" "$repository_dir/logo.png" --out "$icon_dir/AppIcon.iconset/icon_${size}x${size}.png" >/dev/null
    double_size=$((size * 2))
    sips -z "$double_size" "$double_size" "$repository_dir/logo.png" --out "$icon_dir/AppIcon.iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$icon_dir/AppIcon.iconset" -o "$app_dir/Contents/Resources/AppIcon.icns"
codesign --force --deep --sign - "$app_dir"
ditto -c -k --sequesterRsrc --keepParent "$app_dir" "$package_dir/星笺-macOS-$(uname -m).zip"
du -sh "$app_dir" "$package_dir/星笺-macOS-$(uname -m).zip"
