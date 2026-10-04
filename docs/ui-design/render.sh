#!/usr/bin/env bash
# Render every mockup HTML file to a 2x PNG with headless Chrome.
#
# Regenerating requires Chrome at the macOS path below. On other platforms pass
# it in: CHROME=/path/to/chrome ./render.sh
set -uo pipefail

DIR="$(cd "$(dirname "$0")" && pwd)"
OUT="$DIR/images"
HTML="$DIR/out"
CHROME="${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"

if [ ! -x "$CHROME" ]; then
  echo "Chrome not found at: $CHROME" >&2
  echo "Set CHROME=/path/to/chrome and re-run." >&2
  exit 1
fi

mkdir -p "$OUT"
rm -f "$OUT"/*.png

# The generated pages live in out/, so the stylesheet has to sit beside them.
cp "$DIR/forge.css" "$HTML/forge.css"

render() {
  local name="$1" width="$2" height="$3"
  "$CHROME" --headless --disable-gpu --hide-scrollbars \
    --force-device-scale-factor=2 \
    --virtual-time-budget=3000 \
    --window-size="${width},${height}" \
    --screenshot="$OUT/${name}.png" \
    "file://$HTML/${name}.html" >/dev/null 2>&1
  if [ -f "$OUT/${name}.png" ]; then
    printf "  ok   %-26s %s\n" "$name" "$(du -h "$OUT/${name}.png" | cut -f1)"
  else
    printf "  FAIL %-26s\n" "$name"
  fi
}

echo "Rendering mockups at 2x..."

# Heights are tuned to the content each page actually has; an over-tall window
# would just pad the bottom with dead space in the screenshot.
render 01-landing           1440 3050
render 02-register          1440 880
render 03-sign-in           1440 880
render 04-dashboard         1600 1150
render 05-jobs              1600 660
render 06-job-builder       1600 1130
render 07-why-didnt-run      1600 1010
render 08-executions        1600 700
render 09-execution-detail  1600 1290
render 10-running-now       1600 700
render 11-calendar          1600 790
render 12-workflows         1600 660
render 13-emergency         1600 780
render 14-onboarding        1600 900
render 15-states            1600 1130
render 16-command-palette   1440 700
render 17-job-detail        1600 1210

echo
echo "PNG count: $(ls -1 "$OUT"/*.png 2>/dev/null | wc -l | tr -d ' ')"