#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SOURCE_SVG="$REPO_ROOT/assets/branding/actro-mark.svg"
DEFAULT_BACKGROUND="#F3F6FA"
DARK_BACKGROUND="#101522"
MARK_ROTATION_DEGREES="30"

usage() {
  printf 'Usage: %s [--source PATH] [--background HEX] [--dark-background HEX] [--rotation DEGREES]\n' "$0"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --source)
      SOURCE_SVG="${2:?--source requires a path}"
      shift 2
      ;;
    --background)
      DEFAULT_BACKGROUND="${2:?--background requires a color}"
      shift 2
      ;;
    --dark-background)
      DARK_BACKGROUND="${2:?--dark-background requires a color}"
      shift 2
      ;;
    --rotation)
      MARK_ROTATION_DEGREES="${2:?--rotation requires clockwise degrees}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      printf 'Unknown option: %s\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

for required_tool in rsvg-convert magick; do
  if ! command -v "$required_tool" >/dev/null 2>&1; then
    printf 'Missing required tool: %s\n' "$required_tool" >&2
    exit 1
  fi
done

if [[ ! -f "$SOURCE_SVG" ]]; then
  printf 'SVG source does not exist: %s\n' "$SOURCE_SVG" >&2
  exit 1
fi

WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/actro-app-icons.XXXXXX")"
trap 'rm -rf "$WORK_DIR"' EXIT

IOS_SET="$REPO_ROOT/apps/ios/actro/Actro/Assets.xcassets/AppIcon.appiconset"
MAC_SET="$REPO_ROOT/apps/macos/actro/Actro/Assets.xcassets/AppIcon.appiconset"
ANDROID_RES="$REPO_ROOT/apps/android/actro/android/app/src/main/res"
ANDROID_MAIN="$REPO_ROOT/apps/android/actro/android/app/src/main"
WINDOWS_ASSETS="$REPO_ROOT/apps/windows/actro/assets"
BRANDING_GENERATED="$REPO_ROOT/assets/branding/generated"

mkdir -p "$IOS_SET" "$MAC_SET" "$WINDOWS_ASSETS" "$BRANDING_GENERATED"

render_mark() {
  local size="$1"
  local output="$2"
  magick "$SOURCE_RENDER" -background none -rotate "$MARK_ROTATION_DEGREES" \
    -trim +repage -filter Lanczos -resize "${size}x${size}" -strip "$output"
}

resize_png() {
  local source="$1"
  local size="$2"
  local output="$3"
  magick "$source" -filter Lanczos -resize "${size}x${size}" -strip "$output"
}

IOS_MARK="$WORK_DIR/mark-ios.png"
MAC_MARK="$WORK_DIR/mark-macos.png"
ANDROID_MARK="$WORK_DIR/mark-android.png"
SOURCE_RENDER="$WORK_DIR/mark-source.png"
rsvg-convert --keep-aspect-ratio --width 2048 --height 2048 \
  "$SOURCE_SVG" --output "$SOURCE_RENDER"
render_mark 960 "$IOS_MARK"
render_mark 940 "$MAC_MARK"
render_mark 660 "$ANDROID_MARK"

IOS_DEFAULT="$WORK_DIR/ios-default.png"
IOS_DARK="$WORK_DIR/ios-dark.png"
IOS_TINTED="$WORK_DIR/ios-tinted.png"
TINTED_MARK="$WORK_DIR/mark-tinted.png"

LIGHT_BACKGROUND="$WORK_DIR/background-light.png"
DARK_BACKGROUND_IMAGE="$WORK_DIR/background-dark.png"
TINTED_BACKGROUND="$WORK_DIR/background-tinted.png"
magick -size 1024x1024 "xc:$DEFAULT_BACKGROUND" -strip "$LIGHT_BACKGROUND"
magick -size 1024x1024 "xc:$DARK_BACKGROUND" -strip "$DARK_BACKGROUND_IMAGE"
magick -size 1024x1024 xc:'#ECEFF4' -strip "$TINTED_BACKGROUND"

magick "$LIGHT_BACKGROUND" "$IOS_MARK" \
  -gravity center -composite -alpha off -strip "$IOS_DEFAULT"
magick "$DARK_BACKGROUND_IMAGE" "$IOS_MARK" \
  -gravity center -composite -alpha off -strip "$IOS_DARK"
magick "$IOS_MARK" -colorspace Gray -auto-level "$TINTED_MARK"
magick "$TINTED_BACKGROUND" "$TINTED_MARK" \
  -gravity center -composite -alpha off -strip "$IOS_TINTED"

cp "$IOS_DEFAULT" "$BRANDING_GENERATED/actro-icon-default-1024.png"
cp "$IOS_DARK" "$BRANDING_GENERATED/actro-icon-dark-1024.png"
cp "$IOS_TINTED" "$BRANDING_GENERATED/actro-icon-tinted-1024.png"
magick -size 1024x1024 xc:none "$IOS_MARK" -gravity center -composite \
  -strip "$BRANDING_GENERATED/actro-mark-transparent-1024.png"

cp "$IOS_DEFAULT" "$IOS_SET/AppIcon-1024.png"
cp "$IOS_DARK" "$IOS_SET/AppIcon-1024-dark.png"
cp "$IOS_TINTED" "$IOS_SET/AppIcon-1024-tinted.png"

MAC_MASTER="$WORK_DIR/macos-master.png"
MAC_MASK="$WORK_DIR/macos-mask.png"
MAC_BACKGROUND="$WORK_DIR/macos-background.png"
magick -size 1024x1024 xc:black -fill white \
  -draw 'roundrectangle 56,56 968,968 205,205' "$MAC_MASK"
magick "$LIGHT_BACKGROUND" "$MAC_MASK" -alpha off -compose CopyOpacity \
  -composite "$MAC_BACKGROUND"
magick "$MAC_BACKGROUND" "$MAC_MARK" -gravity center -composite -strip "$MAC_MASTER"

make_mac_slots() {
  local destination="$1"
  resize_png "$MAC_MASTER" 16 "$destination/AppIcon-mac-16.png"
  resize_png "$MAC_MASTER" 32 "$destination/AppIcon-mac-16@2x.png"
  resize_png "$MAC_MASTER" 32 "$destination/AppIcon-mac-32.png"
  resize_png "$MAC_MASTER" 64 "$destination/AppIcon-mac-32@2x.png"
  resize_png "$MAC_MASTER" 128 "$destination/AppIcon-mac-128.png"
  resize_png "$MAC_MASTER" 256 "$destination/AppIcon-mac-128@2x.png"
  resize_png "$MAC_MASTER" 256 "$destination/AppIcon-mac-256.png"
  resize_png "$MAC_MASTER" 512 "$destination/AppIcon-mac-256@2x.png"
  resize_png "$MAC_MASTER" 512 "$destination/AppIcon-mac-512.png"
  resize_png "$MAC_MASTER" 1024 "$destination/AppIcon-mac-512@2x.png"
}

make_mac_slots "$IOS_SET"
make_mac_slots "$MAC_SET"

write_ios_contents() {
  local destination="$1"
  cat > "$destination/Contents.json" <<'JSON'
{
  "images" : [
    { "filename" : "AppIcon-1024.png", "idiom" : "universal", "platform" : "ios", "size" : "1024x1024" },
    { "appearances" : [{ "appearance" : "luminosity", "value" : "dark" }], "filename" : "AppIcon-1024-dark.png", "idiom" : "universal", "platform" : "ios", "size" : "1024x1024" },
    { "appearances" : [{ "appearance" : "luminosity", "value" : "tinted" }], "filename" : "AppIcon-1024-tinted.png", "idiom" : "universal", "platform" : "ios", "size" : "1024x1024" },
    { "filename" : "AppIcon-mac-16.png", "idiom" : "mac", "scale" : "1x", "size" : "16x16" },
    { "filename" : "AppIcon-mac-16@2x.png", "idiom" : "mac", "scale" : "2x", "size" : "16x16" },
    { "filename" : "AppIcon-mac-32.png", "idiom" : "mac", "scale" : "1x", "size" : "32x32" },
    { "filename" : "AppIcon-mac-32@2x.png", "idiom" : "mac", "scale" : "2x", "size" : "32x32" },
    { "filename" : "AppIcon-mac-128.png", "idiom" : "mac", "scale" : "1x", "size" : "128x128" },
    { "filename" : "AppIcon-mac-128@2x.png", "idiom" : "mac", "scale" : "2x", "size" : "128x128" },
    { "filename" : "AppIcon-mac-256.png", "idiom" : "mac", "scale" : "1x", "size" : "256x256" },
    { "filename" : "AppIcon-mac-256@2x.png", "idiom" : "mac", "scale" : "2x", "size" : "256x256" },
    { "filename" : "AppIcon-mac-512.png", "idiom" : "mac", "scale" : "1x", "size" : "512x512" },
    { "filename" : "AppIcon-mac-512@2x.png", "idiom" : "mac", "scale" : "2x", "size" : "512x512" }
  ],
  "info" : { "author" : "xcode", "version" : 1 }
}
JSON
}

write_macos_contents() {
  local destination="$1"
  cat > "$destination/Contents.json" <<'JSON'
{
  "images" : [
    { "filename" : "AppIcon-mac-16.png", "idiom" : "mac", "scale" : "1x", "size" : "16x16" },
    { "filename" : "AppIcon-mac-16@2x.png", "idiom" : "mac", "scale" : "2x", "size" : "16x16" },
    { "filename" : "AppIcon-mac-32.png", "idiom" : "mac", "scale" : "1x", "size" : "32x32" },
    { "filename" : "AppIcon-mac-32@2x.png", "idiom" : "mac", "scale" : "2x", "size" : "32x32" },
    { "filename" : "AppIcon-mac-128.png", "idiom" : "mac", "scale" : "1x", "size" : "128x128" },
    { "filename" : "AppIcon-mac-128@2x.png", "idiom" : "mac", "scale" : "2x", "size" : "128x128" },
    { "filename" : "AppIcon-mac-256.png", "idiom" : "mac", "scale" : "1x", "size" : "256x256" },
    { "filename" : "AppIcon-mac-256@2x.png", "idiom" : "mac", "scale" : "2x", "size" : "256x256" },
    { "filename" : "AppIcon-mac-512.png", "idiom" : "mac", "scale" : "1x", "size" : "512x512" },
    { "filename" : "AppIcon-mac-512@2x.png", "idiom" : "mac", "scale" : "2x", "size" : "512x512" }
  ],
  "info" : { "author" : "xcode", "version" : 1 }
}
JSON
}

write_ios_contents "$IOS_SET"
write_macos_contents "$MAC_SET"

ANDROID_FOREGROUND_MASTER="$WORK_DIR/android-foreground.png"
ANDROID_MONOCHROME_MASTER="$WORK_DIR/android-monochrome.png"
ANDROID_ROUND_MASTER="$WORK_DIR/android-round.png"
ANDROID_ALPHA_MASK="$WORK_DIR/android-alpha-mask.png"
ANDROID_ROUND_MASK="$WORK_DIR/android-round-mask.png"
ANDROID_ROUND_BACKGROUND="$WORK_DIR/android-round-background.png"

magick -size 1080x1080 xc:none "$ANDROID_MARK" -gravity center -composite \
  -strip "$ANDROID_FOREGROUND_MASTER"
magick "$ANDROID_FOREGROUND_MASTER" -alpha extract "$ANDROID_ALPHA_MASK"
magick -size 1080x1080 xc:white "$ANDROID_ALPHA_MASK" \
  -alpha off -compose CopyOpacity -composite -strip "$ANDROID_MONOCHROME_MASTER"
magick -size 1024x1024 xc:black -fill white \
  -draw 'circle 512,512 512,32' "$ANDROID_ROUND_MASK"
magick "$LIGHT_BACKGROUND" "$ANDROID_ROUND_MASK" -alpha off \
  -compose CopyOpacity -composite "$ANDROID_ROUND_BACKGROUND"
magick "$ANDROID_ROUND_BACKGROUND" "$MAC_MARK" -gravity center -composite \
  -strip "$ANDROID_ROUND_MASTER"

ANDROID_DENSITIES=(mdpi hdpi xhdpi xxhdpi xxxhdpi)
ANDROID_LEGACY_SIZES=(48 72 96 144 192)
ANDROID_FOREGROUND_SIZES=(108 162 216 324 432)

for index in "${!ANDROID_DENSITIES[@]}"; do
  density="${ANDROID_DENSITIES[$index]}"
  legacy_size="${ANDROID_LEGACY_SIZES[$index]}"
  foreground_size="${ANDROID_FOREGROUND_SIZES[$index]}"
  destination="$ANDROID_RES/mipmap-$density"
  mkdir -p "$destination"
  resize_png "$IOS_DEFAULT" "$legacy_size" "$destination/ic_launcher.png"
  resize_png "$ANDROID_ROUND_MASTER" "$legacy_size" "$destination/ic_launcher_round.png"
  resize_png "$ANDROID_FOREGROUND_MASTER" "$foreground_size" "$destination/ic_launcher_foreground.png"
  resize_png "$ANDROID_MONOCHROME_MASTER" "$foreground_size" "$destination/ic_launcher_monochrome.png"
done

mkdir -p "$ANDROID_RES/mipmap-anydpi-v26" "$ANDROID_RES/mipmap-anydpi-v33" \
  "$ANDROID_RES/drawable" "$ANDROID_RES/drawable-night"
cat > "$ANDROID_RES/drawable/ic_launcher_background.xml" <<'XML'
<?xml version="1.0" encoding="utf-8"?>
<shape xmlns:android="http://schemas.android.com/apk/res/android" android:shape="rectangle">
    <solid android:color="#F3F6FA" />
</shape>
XML
cat > "$ANDROID_RES/drawable-night/ic_launcher_background.xml" <<'XML'
<?xml version="1.0" encoding="utf-8"?>
<shape xmlns:android="http://schemas.android.com/apk/res/android" android:shape="rectangle">
    <solid android:color="#101522" />
</shape>
XML

for launcher_name in ic_launcher ic_launcher_round; do
  cat > "$ANDROID_RES/mipmap-anydpi-v26/$launcher_name.xml" <<XML
<?xml version="1.0" encoding="utf-8"?>
<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">
    <background android:drawable="@drawable/ic_launcher_background" />
    <foreground android:drawable="@mipmap/ic_launcher_foreground" />
</adaptive-icon>
XML
  cat > "$ANDROID_RES/mipmap-anydpi-v33/$launcher_name.xml" <<XML
<?xml version="1.0" encoding="utf-8"?>
<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">
    <background android:drawable="@drawable/ic_launcher_background" />
    <foreground android:drawable="@mipmap/ic_launcher_foreground" />
    <monochrome android:drawable="@mipmap/ic_launcher_monochrome" />
</adaptive-icon>
XML
done

resize_png "$IOS_DEFAULT" 512 "$ANDROID_MAIN/play-store-icon.png"

WINDOWS_MASTER="$WORK_DIR/windows-master.png"
cp "$IOS_DEFAULT" "$WINDOWS_MASTER"
for windows_size in 16 20 24 32 40 48 64 128 256 512; do
  resize_png "$WINDOWS_MASTER" "$windows_size" "$WINDOWS_ASSETS/Actro-${windows_size}.png"
done
magick "$WINDOWS_MASTER" -define icon:auto-resize=256,128,64,48,40,32,24,20,16 \
  "$WINDOWS_ASSETS/Actro.ico"

for required_png in \
  "$IOS_SET/AppIcon-1024.png" \
  "$IOS_SET/AppIcon-1024-dark.png" \
  "$IOS_SET/AppIcon-1024-tinted.png" \
  "$MAC_SET/AppIcon-mac-512@2x.png" \
  "$ANDROID_RES/mipmap-xxxhdpi/ic_launcher_foreground.png" \
  "$ANDROID_MAIN/play-store-icon.png" \
  "$WINDOWS_ASSETS/Actro-256.png"; do
  dimensions="$(magick identify -format '%wx%h' "$required_png")"
  if [[ -z "$dimensions" ]]; then
    printf 'Generated image could not be read: %s\n' "$required_png" >&2
    exit 1
  fi
done

if [[ ! -s "$WINDOWS_ASSETS/Actro.ico" ]]; then
  printf 'Windows ICO was not generated\n' >&2
  exit 1
fi

printf 'Generated Actro application icons from %s\n' "$SOURCE_SVG"
printf '  iOS:    %s\n' "$IOS_SET"
printf '  macOS:  %s\n' "$MAC_SET"
printf '  Android:%s\n' "$ANDROID_RES"
printf '  Windows:%s\n' "$WINDOWS_ASSETS"
