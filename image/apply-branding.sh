#!/usr/bin/env bash
# Apply DARKJUANDER branding during the bootc image build.
# CI runs this inside the image build. Nothing here has been booted.
set -euo pipefail

die() {
  echo "error: $*" >&2
  exit 1
}

need() {
  local path=$1
  if [[ ! -s "${path}" ]]; then
    die "branding asset missing or empty: ${path}"
  fi
}

[[ -e /usr/lib64/plymouth/script.so || -e /usr/lib/plymouth/script.so ]] \
  || die "plymouth script module is missing (package plymouth-plugin-script)"
[[ -e /usr/lib64/plymouth/label-freetype.so || -e /usr/lib/plymouth/label-freetype.so ]] \
  || die "plymouth label module is missing (package plymouth-plugin-label)"
command -v plymouth-set-default-theme >/dev/null 2>&1 \
  || die "plymouth-set-default-theme is not installed (package plymouth-scripts)"

theme_dir=/usr/share/plymouth/themes/darkjuander
for asset in \
  darkjuander.plymouth \
  darkjuander.script \
  logo.png \
  wordmark.png \
  progress_box.png \
  progress_bar.png \
  entry.png \
  bullet.png \
  lock.png
do
  need "${theme_dir}/${asset}"
done
grep -q 'Image.Text(prompt_text' "${theme_dir}/darkjuander.script" \
  || die "plymouth script does not draw prompt_text"
grep -q 'Inter 14' "${theme_dir}/darkjuander.script" \
  || die "plymouth script does not use Inter 14"

plymouth-set-default-theme darkjuander

selected=0
if [[ -f /etc/plymouth/plymouthd.conf ]] && grep -q '^Theme=darkjuander$' /etc/plymouth/plymouthd.conf; then
  selected=1
fi
default_link=/usr/share/plymouth/themes/default.plymouth
if [[ -L "${default_link}" ]]; then
  default_target=$(readlink -f "${default_link}")
  if [[ "${default_target}" == "${theme_dir}/darkjuander.plymouth" ]]; then
    selected=1
  fi
fi
if ((selected == 0)); then
  echo "plymouth-set-default-theme did not select darkjuander" >&2
  if [[ -f /etc/plymouth/plymouthd.conf ]]; then
    echo "--- /etc/plymouth/plymouthd.conf ---" >&2
    cat /etc/plymouth/plymouthd.conf >&2
  fi
  if [[ -e "${default_link}" ]]; then
    echo "--- ${default_link} ---" >&2
    ls -l "${default_link}" >&2
  fi
  exit 1
fi

need /usr/lib/dracut/dracut.conf.d/darkjuander-plymouth.conf
grep -q 'add_dracutmodules+=" plymouth "' /usr/lib/dracut/dracut.conf.d/darkjuander-plymouth.conf \
  || die "dracut config does not add the plymouth module"
grep -q 'Inter.ttf' /usr/lib/dracut/dracut.conf.d/darkjuander-plymouth.conf \
  || die "dracut config does not install Inter"

kargs=/usr/lib/bootc/kargs.d/10-darkjuander.toml
need "${kargs}"
grep -q 'rhgb' "${kargs}" || die "${kargs} does not set rhgb"
grep -q 'quiet' "${kargs}" || die "${kargs} does not set quiet"

font_dir=/usr/share/fonts/darkjuander
for asset in \
  Inter.ttf \
  JetBrainsMono.ttf \
  SpaceGrotesk.ttf \
  OFL-Inter.txt \
  OFL-JetBrainsMono.txt \
  OFL-SpaceGrotesk.txt
do
  need "${font_dir}/${asset}"
done
command -v python3 >/dev/null 2>&1 \
  || die "python3 is not installed (needed to instantiate Inter for Plymouth)"
python3 - "${font_dir}" <<'PY'
import sys
from pathlib import Path

from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

src = Path(sys.argv[1])
inter = src / "Inter.ttf"
if not inter.is_file() or inter.stat().st_size == 0:
    sys.exit(f"error: font missing or empty: {inter}")
dest = src / "Inter-Regular.ttf"
instancer.instantiateVariableFont(TTFont(inter), {"wght": 400, "opsz": 14}).save(str(dest))
if dest.stat().st_size == 0:
    sys.exit(f"error: font instance is empty: {dest}")
print(f"instantiated {dest}")
PY
need "${font_dir}/Inter-Regular.ttf"
need /usr/share/fontconfig/conf.avail/50-darkjuander-inter.conf
need /etc/fonts/conf.d/50-darkjuander-inter.conf
if [[ -e /etc/fonts/conf.d/60-darkjuander-inter.conf ]]; then
  die "fontconfig file 60-darkjuander-inter.conf sorts after Noto and must not be installed"
fi
fc-cache -f
default_font=$(fc-match -f '%{file}\n')
printf '%s\n' "${default_font}" | grep -q 'Inter' \
  || die "fc-match default font is ${default_font}, not Inter"

bg_dir=/usr/share/backgrounds/darkjuander
for asset in \
  darkjuander-3840x2160.png \
  darkjuander-2560x1600.png \
  darkjuander-1920x1080.png
do
  need "${bg_dir}/${asset}"
done
ln -sfn darkjuander-3840x2160.png "${bg_dir}/default.png"
ln -sfn darkjuander/default.png /usr/share/backgrounds/default.png
[[ -s /usr/share/backgrounds/default.png ]] \
  || die "default background symlink is broken"
need /usr/share/gnome-background-properties/darkjuander.xml
grep -q '/usr/share/backgrounds/darkjuander/darkjuander-3840x2160.png' \
  /usr/share/gnome-background-properties/darkjuander.xml \
  || die "background metadata does not point at the installed wallpaper"

need /usr/share/icons/hicolor/scalable/apps/darkjuander.svg
for size in 16 24 32 48 64 128 256 512; do
  need "/usr/share/icons/hicolor/${size}x${size}/apps/darkjuander.png"
done
install -D -m 0644 /usr/share/icons/hicolor/256x256/apps/darkjuander.png \
  /usr/share/pixmaps/darkjuander.png
need /usr/share/pixmaps/darkjuander.png

rpm -q generic-logos >/dev/null \
  || die "generic-logos is not installed; the image must swap out fedora-logos"
if rpm -q fedora-logos >/dev/null 2>&1; then
  die "fedora-logos is still installed"
fi

term_dir=/usr/share/darkjuander/terminal
for asset in \
  darkjuander.Xresources \
  foot.ini \
  kitty.conf \
  alacritty.toml \
  gnome-terminal.dconf \
  palette.txt
do
  need "${term_dir}/${asset}"
done

os_release=/usr/lib/os-release
[[ -f "${os_release}" ]] || die "missing ${os_release}; refusing to invent an os-release"
python3 - "${os_release}" <<'PY'
import sys
from pathlib import Path

path = Path(sys.argv[1])
repo = "https://github.com/Awkward8787/DARKJUANDER"
wanted = {
    "NAME": '"DARKJUANDER"',
    "PRETTY_NAME": '"DARKJUANDER"',
    "LOGO": "darkjuander",
    "HOME_URL": f'"{repo}"',
    "SUPPORT_URL": f'"{repo}"',
    "BUG_REPORT_URL": f'"{repo}"',
    "ANSI_COLOR": '"0;38;2;157;140;255"',
    "DEFAULT_HOSTNAME": "darkjuander",
}
preserved = ("ID", "VERSION_ID", "VARIANT_ID", "ID_LIKE")

def parse(text):
    got = {}
    for line in text.splitlines():
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        got[key] = value
    return got

before = parse(path.read_text())
if "ID" not in before:
    sys.exit(f"error: {path} has no ID=")
lines = path.read_text().splitlines()
seen = set()
out = []
for line in lines:
    if not line or line.lstrip().startswith("#") or "=" not in line:
        out.append(line)
        continue
    key = line.split("=", 1)[0]
    if key in wanted:
        out.append(f"{key}={wanted[key]}")
        seen.add(key)
    else:
        out.append(line)
for key, value in wanted.items():
    if key not in seen:
        out.append(f"{key}={value}")
text = "\n".join(out) + "\n"
path.write_text(text)
after = parse(text)
for key, value in wanted.items():
    if after.get(key) != value:
        sys.exit(f"error: {path} has {key}={after.get(key)!r}, expected {value}")
for key in preserved:
    if before.get(key) != after.get(key):
        sys.exit(f"error: {path} changed {key} from {before.get(key)!r} to {after.get(key)!r}")
print(f"os-release updated. ID stayed {after['ID']}")
PY

if [[ -L /etc/os-release ]]; then
  os_target=$(readlink -f /etc/os-release)
  [[ "${os_target}" == "${os_release}" ]] \
    || die "/etc/os-release points at ${os_target}, not ${os_release}"
elif [[ -f /etc/os-release ]]; then
  cp -a "${os_release}" /etc/os-release
else
  ln -s ../usr/lib/os-release /etc/os-release
fi
grep -q '^NAME="DARKJUANDER"$' "${os_release}" || die "NAME was not written to ${os_release}"
grep -q '^PRETTY_NAME="DARKJUANDER"$' "${os_release}" || die "PRETTY_NAME was not written"
grep -q '^LOGO=darkjuander$' "${os_release}" || die "LOGO was not written"
grep -q '^HOME_URL="https://github.com/Awkward8787/DARKJUANDER"$' "${os_release}" \
  || die "HOME_URL was not written"
grep -q '^ID=fedora$' "${os_release}" || die "ID=fedora was not kept"
if grep -q '^ID_LIKE=' "${os_release}"; then
  die "ID_LIKE must stay unset while ID=fedora"
fi

# /usr/lib/issue already prints \S, which is PRETTY_NAME. A second
# DARKJUANDER line in issue.d repeats the name before login. The planned
# "Welcome back" sentence is not installed. Do not replace /usr/lib/issue.
rm -f /usr/lib/issue.d/50-darkjuander.issue
if [[ -d /usr/lib/issue.d ]]; then
  while IFS= read -r issue_file; do
    if grep -q 'DARKJUANDER' "${issue_file}"; then
      die "issue.d repeats DARKJUANDER; /usr/lib/issue already prints PRETTY_NAME"
    fi
  done < <(find /usr/lib/issue.d -type f)
fi
if [[ -f /usr/lib/issue ]] && grep -q 'Welcome back. Your agents are where you left them.' /usr/lib/issue; then
  die "refusing to leave the planned login greeting in /usr/lib/issue"
fi
if [[ -f /etc/motd ]] && grep -q 'umbrad: running' /etc/motd; then
  die "refusing to leave a hard-coded motd; the brand guide marks that line planned"
fi

echo "DARKJUANDER branding files are in the image. Plymouth is the default theme. GRUB art is not installed. This has not been booted."
