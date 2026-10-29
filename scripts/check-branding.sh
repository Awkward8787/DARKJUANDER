#!/usr/bin/env bash
# Check the committed brand kit and, with --no-png, run the brand build
# without exporting PNGs. This does not build the bootc image.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "${root}"
run_build=0
run_social=0
if [[ "${1:-}" == "--no-png" ]]; then
  run_build=1
elif [[ "${1:-}" == "--check-social" ]]; then
  run_social=1
elif [[ $# -gt 0 ]]; then
  echo "usage: $0 [--no-png|--check-social]" >&2
  exit 2
fi

python3 - <<'PY'
import json
import sys
from pathlib import Path

path = Path("branding/src/palette.json")
try:
    data = json.loads(path.read_text())
except FileNotFoundError:
    sys.exit(f"error: missing {path}")
except json.JSONDecodeError as exc:
    sys.exit(f"error: {path} is not JSON: {exc}")

required = {
    "brand": ["umbra", "violet", "violet_deep"],
    "neutral": [
        "umbra_950", "umbra_900", "umbra_800", "umbra_700", "umbra_600",
        "umbra_500", "ash_400", "ash_200", "lumen_100", "lumen_50",
    ],
    "semantic_dark": ["ok", "warn", "danger", "info"],
    "semantic_light": ["ok", "warn", "danger", "info"],
    "terminal": [
        "background", "foreground", "cursor", "selection_bg", "selection_fg", "ansi",
    ],
}

def hex_color(value, where):
    if not isinstance(value, str) or len(value) != 7 or value[0] != "#":
        sys.exit(f"error: {where} is not a #RRGGBB color: {value!r}")
    try:
        int(value[1:], 16)
    except ValueError:
        sys.exit(f"error: {where} is not hex: {value!r}")

for group, keys in required.items():
    block = data.get(group)
    if not isinstance(block, dict):
        sys.exit(f"error: palette.json missing object {group}")
    for key in keys:
        if key not in block:
            sys.exit(f"error: palette.json missing {group}.{key}")
        if key == "ansi":
            continue
        hex_color(block[key], f"{group}.{key}")

ansi = data["terminal"]["ansi"]
if not isinstance(ansi, list) or len(ansi) != 16:
    sys.exit(f"error: terminal.ansi must be 16 colors, got {len(ansi) if isinstance(ansi, list) else type(ansi)}")
for index, color in enumerate(ansi):
    hex_color(color, f"terminal.ansi[{index}]")

expected = {
    "umbra": "#0A0C11",
    "violet": "#9D8CFF",
    "violet_deep": "#5B48D6",
}
for key, value in expected.items():
    if data["brand"][key] != value:
        sys.exit(f"error: brand.{key} is {data['brand'][key]}, expected {value}")
if data["neutral"]["umbra_950"] != "#0A0C11":
    sys.exit("error: neutral.umbra_950 changed")
if data["terminal"]["background"] != "#0A0C11" or data["terminal"]["cursor"] != "#9D8CFF":
    sys.exit("error: terminal background or cursor does not match the brand palette")
print("palette.json matches the Umbra Eclipse brand colors")
PY

direction=$(tr -d '[:space:]' < branding/BUILD_DIRECTION)
if [[ "${direction}" != "eclipse" ]]; then
  echo "error: BUILD_DIRECTION is '${direction}', expected eclipse" >&2
  exit 1
fi

need() {
  if [[ ! -s "$1" ]]; then
    echo "error: missing or empty brand file: $1" >&2
    exit 1
  fi
}

need branding/LICENSE-BRAND.md
need branding/brand-guide.md
need branding/logo/eclipse/svg/eclipse-horizontal-light.svg
need branding/logo/eclipse/svg/eclipse-horizontal-dark.svg
need branding/logo/eclipse/svg/eclipse-icon-dark.svg
need branding/logo/eclipse/png/horizontal/eclipse-horizontal-light-1024w.png
need branding/logo/eclipse/png/horizontal/eclipse-horizontal-dark-1024w.png
need branding/logo/tag/svg/tag-horizontal-dark.svg
need branding/logo/lattice/svg/lattice-horizontal-dark.svg
need branding/logo/route/svg/route-horizontal-dark.svg
need branding/plymouth/darkjuander/darkjuander.plymouth
need branding/plymouth/darkjuander/darkjuander.script
need branding/plymouth/darkjuander/logo.svg
need branding/grub/darkjuander/theme.txt
need branding/grub/darkjuander/background.svg
need branding/wallpapers/darkjuander-3840x2160.svg
need branding/wallpapers/darkjuander-2560x1600.svg
need branding/wallpapers/darkjuander-1920x1080.svg
need branding/os-icon/hicolor/scalable/apps/darkjuander.svg
need branding/social/darkjuander-social-1280x640.svg
need branding/terminal/darkjuander.Xresources
need branding/terminal/foot.ini
need branding/terminal/kitty.conf
need branding/terminal/alacritty.toml
need branding/terminal/gnome-terminal.dconf
need branding/src/fonts/Inter.ttf
need branding/src/fonts/JetBrainsMono.ttf
need branding/src/fonts/SpaceGrotesk.ttf
need branding/src/fonts/OFL-Inter.txt
need branding/src/fonts/OFL-JetBrainsMono.txt
need branding/src/fonts/OFL-SpaceGrotesk.txt
need branding/social/darkjuander-social-1280x640.png
social_bytes=$(wc -c < branding/social/darkjuander-social-1280x640.png)
if [[ "${social_bytes}" -ge 1048576 ]]; then
  echo "error: social preview is ${social_bytes} bytes; it must be under 1 MB" >&2
  exit 1
fi
python3 - <<'PY'
import struct
import sys
from pathlib import Path

path = Path("branding/social/darkjuander-social-1280x640.png")
data = path.read_bytes()
if data[:8] != b"\x89PNG\r\n\x1a\n":
    sys.exit("error: social preview is not a PNG")
width, height = struct.unpack(">II", data[16:24])
if (width, height) != (1280, 640):
    sys.exit(f"error: social preview is {width}x{height}, expected 1280x640")
print(f"social preview is 1280x640 and {path.stat().st_size} bytes")
PY

preview_count=$(find branding/logo/tag branding/logo/lattice branding/logo/route -type f -name '*.png' | wc -l)
if [[ "${preview_count}" -ne 0 ]]; then
  echo "error: tag, lattice, and route PNGs are generated by build.py and must not be committed (found ${preview_count})" >&2
  find branding/logo/tag branding/logo/lattice branding/logo/route -type f -name '*.png' >&2
  exit 1
fi

oversized=$(find branding -type f -size +2M -print)
if [[ -n "${oversized}" ]]; then
  echo "error: brand file over 2 MB; compress it or regenerate it from source" >&2
  echo "${oversized}" >&2
  exit 1
fi

if git ls-files -- 'branding/src/fonts/.instances' | grep -q .; then
  echo "error: branding/src/fonts/.instances is a build cache and must not be committed" >&2
  exit 1
fi
if git ls-files -- 'branding/grub/darkjuander/*.pf2' | grep -q .; then
  echo "error: GRUB .pf2 fonts are built in the image and must not be committed" >&2
  exit 1
fi

grep -q 'Agents inside a boundary you can audit.' README.md \
  || { echo "error: README is missing the tagline" >&2; exit 1; }
grep -q 'branding/logo/eclipse/png/horizontal/eclipse-horizontal-light-1024w.png' README.md \
  || { echo "error: README logo is not the committed eclipse horizontal light PNG" >&2; exit 1; }
grep -q 'branding/logo/eclipse/png/horizontal/eclipse-horizontal-dark-1024w.png' README.md \
  || { echo "error: README logo is missing the eclipse horizontal dark PNG" >&2; exit 1; }
python3 - <<'PY'
import re
import struct
import sys
from pathlib import Path

readme = Path("README.md").read_text()
match = re.search(r"<img\b([^>]*)>", readme)
if not match:
    sys.exit("error: README has no logo img")
attrs = dict(re.findall(r'([A-Za-z]+)="([^"]*)"', match.group(1)))
src = attrs.get("src", "")
if not src.endswith("eclipse-horizontal-light-1024w.png"):
    sys.exit(f"error: README logo src is {src}")
try:
    width = int(attrs["width"])
    height = int(attrs["height"])
except KeyError as exc:
    sys.exit(f"error: README logo img is missing {exc.args[0]}")
data = Path(src).read_bytes()
png_w, png_h = struct.unpack(">II", data[16:24])
expected = round(width * png_h / png_w)
if height != expected:
    sys.exit(f"error: README logo height {height} does not match width {width} at {png_w}x{png_h} (expected {expected})")
print(f"README logo is {width}x{height} for a {png_w}x{png_h} PNG")
PY
grep -q 'darkjuander-social-1280x640.png' branding/README.md \
  || { echo "error: branding README does not name the social preview" >&2; exit 1; }
grep -q 'GitHub Settings > Social preview' branding/README.md \
  || { echo "error: branding README does not point at GitHub Settings > Social preview" >&2; exit 1; }
grep -q 'branding/LICENSE-BRAND.md' README.md \
  || { echo "error: README does not link the brand license" >&2; exit 1; }
if grep -q '®' README.md; then
  echo "error: README must not mark DARKJUANDER as a registered trademark" >&2
  exit 1
fi

if [[ "${run_build}" -eq 1 ]]; then
  python3 branding/scripts/build.py --no-png
  # build.py writes .pf2 files when grub-mkfont is installed. The image does
  # not install a GRUB theme, and the repository does not keep the fonts.
  find branding/grub -name '*.pf2' -delete
  built=$(tr -d '[:space:]' < branding/BUILD_DIRECTION)
  if [[ "${built}" != "eclipse" ]]; then
    echo "error: build.py --no-png left BUILD_DIRECTION as '${built}'" >&2
    exit 1
  fi
  echo "branding/scripts/build.py --no-png completed for eclipse"
fi

if [[ "${run_social}" -eq 1 ]]; then
  if ! command -v rsvg-convert >/dev/null 2>&1; then
    echo "error: rsvg-convert is required to check the social preview" >&2
    exit 1
  fi
  python3 branding/scripts/build.py --only social
  social_png=branding/social/darkjuander-social-1280x640.png
  social_svg=branding/social/darkjuander-social-1280x640.svg
  for social_file in "${social_png}" "${social_svg}"; do
    if ! git cat-file -e "HEAD:${social_file}" 2>/dev/null; then
      echo "error: ${social_file} is not committed" >&2
      exit 1
    fi
  done
  if ! git diff --exit-code HEAD -- "${social_png}" "${social_svg}"; then
    echo "error: social preview is out of date. Run: python3 branding/scripts/build.py --only social" >&2
    exit 1
  fi
  echo "social preview matches branding/scripts/build.py"
fi

echo "brand kit checks passed (default direction: eclipse)"
