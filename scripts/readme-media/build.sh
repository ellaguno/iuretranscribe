#!/usr/bin/env bash
# Turns the captures in out/<lang>/ into the README media in docs/media/.
#   scripts/readme-media/build.sh [en es]
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
MEDIA="$HERE/../../docs/media"
mkdir -p "$MEDIA"
for lang1 in ${*:-en es}; do
  {
    src="$HERE/out/$lang1"
    # Screenshots: max 1600 px wide, quantized with pngquant.
    for shot in queue minutes vocabulary assistants summary-dark; do
      [ -f "$src/$shot.png" ] || continue
      convert "$src/$shot.png" -resize '1600x>' -strip "$MEDIA/$shot-$lang1.png"
      pngquant --quality 70-90 --speed 1 --force --output "$MEDIA/$shot-$lang1.png" "$MEDIA/$shot-$lang1.png"
    done
    # Hero GIF: ffconcat frame list → 900 px, 12 fps, two-pass palette.
    if [ -f "$src/hero/frames.txt" ]; then
      filters="fps=12,scale=900:-1:flags=lanczos"
      ffmpeg -v error -y -f concat -safe 0 -i "$src/hero/frames.txt" -vf "$filters,palettegen=stats_mode=diff" "$src/hero/palette.png"
      ffmpeg -v error -y -f concat -safe 0 -i "$src/hero/frames.txt" -i "$src/hero/palette.png" \
        -lavfi "$filters [x]; [x][1:v] paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle" -loop 0 "$MEDIA/hero-$lang1.gif"
    fi
  }
done
ls -la "$MEDIA"
