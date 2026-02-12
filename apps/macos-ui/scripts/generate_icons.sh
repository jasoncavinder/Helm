#!/bin/bash
set -e

SOURCE_IMG="design/logo-processed.png"
DEST_DIR="apps/macos-ui/Helm/Assets.xcassets/AppIcon.appiconset"

if [ ! -f "$SOURCE_IMG" ]; then
    echo "Error: Processed logo not found at $SOURCE_IMG"
    exit 1
fi

mkdir -p "$DEST_DIR"

echo "Generating icons..."

# Define sizes
declare -a SIZES=(16 32 64 128 256 512 1024)

for size in "${SIZES[@]}"; do
    magick "$SOURCE_IMG" -resize "${size}x${size}" -gravity center -background transparent -extent "${size}x${size}" "$DEST_DIR/icon_${size}x${size}.png"
done

# Create Contents.json
cat > "$DEST_DIR/Contents.json" <<EOF
{
  "images" : [
    {
      "size" : "16x16",
      "idiom" : "mac",
      "filename" : "icon_16x16.png",
      "scale" : "1x"
    },
    {
      "size" : "16x16",
      "idiom" : "mac",
      "filename" : "icon_32x32.png",
      "scale" : "2x"
    },
    {
      "size" : "32x32",
      "idiom" : "mac",
      "filename" : "icon_32x32.png",
      "scale" : "1x"
    },
    {
      "size" : "32x32",
      "idiom" : "mac",
      "filename" : "icon_64x64.png",
      "scale" : "2x"
    },
    {
      "size" : "128x128",
      "idiom" : "mac",
      "filename" : "icon_128x128.png",
      "scale" : "1x"
    },
    {
      "size" : "128x128",
      "idiom" : "mac",
      "filename" : "icon_256x256.png",
      "scale" : "2x"
    },
    {
      "size" : "256x256",
      "idiom" : "mac",
      "filename" : "icon_256x256.png",
      "scale" : "1x"
    },
    {
      "size" : "256x256",
      "idiom" : "mac",
      "filename" : "icon_512x512.png",
      "scale" : "2x"
    },
    {
      "size" : "512x512",
      "idiom" : "mac",
      "filename" : "icon_512x512.png",
      "scale" : "1x"
    },
    {
      "size" : "512x512",
      "idiom" : "mac",
      "filename" : "icon_1024x1024.png",
      "scale" : "2x"
    }
  ],
  "info" : {
    "version" : 1,
    "author" : "xcode"
  }
}
EOF

echo "AppIcon generation complete."
