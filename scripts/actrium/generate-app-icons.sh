#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
source_svg="$repo_root/ios/Actrium/Resources/actrium-logo.icon/Assets/color-and-text-restored.svg"

for tool in rsvg-convert magick python3; do
    command -v "$tool" >/dev/null || { echo "Missing $tool" >&2; exit 1; }
done

temp_dir="$(mktemp -d)"
trap 'rm -r "$temp_dir"' EXIT
rsvg-convert -w 1024 -h 1024 "$source_svg" -o "$temp_dir/source.png"

mac_icon_dir="$repo_root/macos/Actrium/Assets.xcassets/AppIcon.appiconset"
for size in 16 32 64 128 256 512 1024; do
    magick "$temp_dir/source.png" -resize "${size}x${size}" "$mac_icon_dir/icon-${size}.png"
done

python3 - "$mac_icon_dir/Contents.json" <<'PY'
import json
import sys

images = []
for size in (16, 32, 128, 256, 512):
    for scale in (1, 2):
        images.append({
            "filename": f"icon-{size * scale}.png",
            "idiom": "mac",
            "scale": f"{scale}x",
            "size": f"{size}x{size}",
        })
with open(sys.argv[1], "w", encoding="utf-8") as output:
    json.dump({"images": images, "info": {"author": "xcode", "version": 1}}, output, indent=2)
    output.write("\n")
PY

# The current iOS .icon package already uses this logo. Legacy Android
# launchers need raster resources at each density; the transparent mark is
# composited onto the same dark background used by the iOS icon package.
android_res="$repo_root/android/android/app/src/main/res"
for entry in mdpi:48 hdpi:72 xhdpi:96 xxhdpi:144 xxxhdpi:192; do
    density="${entry%%:*}"
    size="${entry##*:}"
    mkdir -p "$android_res/mipmap-$density"
    magick "$temp_dir/source.png" -resize "${size}x${size}" \
        -background '#101010' -alpha remove -alpha off \
        "$android_res/mipmap-$density/ic_launcher.png"
done

windows_icon_dir="$repo_root/windows/assets"
mkdir -p "$windows_icon_dir"
for size in 16 24 32 48 64 128 256; do
    magick "$temp_dir/source.png" -resize "${size}x${size}" \
        -background '#101010' -alpha remove -alpha off \
        "$temp_dir/windows-${size}.png"
done
magick "$temp_dir"/windows-*.png "$windows_icon_dir/Actrium.ico"
