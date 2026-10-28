#!/usr/bin/env bash
# Apply DARKJUANDER branding during the bootc image build.
# Nothing here has been booted. A missing asset is a build error.
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

command -v plymouth-set-default-theme >/dev/null 2>&1 \
  || die "plymouth-set-default-theme is not installed (package plymouth)"
command -v grub2-mkfont >/dev/null 2>&1 \
  || die "grub2-mkfont is not installed (package grub2-tools-extra)"
command -v python3 >/dev/null 2>&1 \
  || die "python3 is not installed (needed to instantiate OFL fonts for grub2-mkfont)"

shopt -s nullglob
plymouth_plugins=(/usr/lib64/plymouth/script.so /usr/lib/plymouth/script.so)
shopt -u nullglob
if ((${#plymouth_plugins[@]} == 0)); then
  die "plymouth script module is missing (package plymouth-scripts)"
fi

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

instance_dir=/tmp/darkjuander-font-instances
rm -rf "${instance_dir}"
python3 - "${font_dir}" "${instance_dir}" <<'PY'
import sys
from pathlib import Path

from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

src = Path(sys.argv[1])
out = Path(sys.argv[2])
out.mkdir(parents=True, exist_ok=True)

def make(src_name, dest_name, **axes):
    src_path = src / src_name
    if not src_path.is_file() or src_path.stat().st_size == 0:
        sys.exit(f"error: font missing or empty: {src_path}")
    dest = out / dest_name
    instancer.instantiateVariableFont(TTFont(src_path), axes).save(str(dest))
    if dest.stat().st_size == 0:
        sys.exit(f"error: font instance is empty: {dest}")
    print(f"instantiated {dest}")

make("Inter.ttf", "Inter-opsz14-wght400.ttf", wght=400, opsz=14)
make("JetBrainsMono.ttf", "JetBrainsMono-wght400.ttf", wght=400)
PY

grub_dir=/usr/share/grub/themes/darkjuander
need "${grub_dir}/theme.txt"
need "${grub_dir}/background.png"
grep -q 'Inter Regular 18' "${grub_dir}/theme.txt" \
  || die "GRUB theme.txt does not name Inter Regular 18"
grep -q 'JetBrains Mono Regular 16' "${grub_dir}/theme.txt" \
  || die "GRUB theme.txt does not name JetBrains Mono Regular 16"

# Do not pass -n. grub2-mkfont appends the style itself, and these
# instances already produce "Inter Regular 18" and "JetBrains Mono Regular 16",
# which are the names in theme.txt. No boot has confirmed GRUB loads them.
grub2-mkfont -s 18 \
  -o "${grub_dir}/inter_regular_18.pf2" \
  "${instance_dir}/Inter-opsz14-wght400.ttf"
grub2-mkfont -s 16 \
  -o "${grub_dir}/jetbrains_mono_16.pf2" \
  "${instance_dir}/JetBrainsMono-wght400.ttf"
rm -rf "${instance_dir}"

need "${grub_dir}/inter_regular_18.pf2"
need "${grub_dir}/jetbrains_mono_16.pf2"
grep -a -q 'Inter Regular 18' "${grub_dir}/inter_regular_18.pf2" \
  || die "inter_regular_18.pf2 does not contain the theme font name Inter Regular 18"
grep -a -q 'JetBrains Mono Regular 16' "${grub_dir}/jetbrains_mono_16.pf2" \
  || die "jetbrains_mono_16.pf2 does not contain the theme font name JetBrains Mono Regular 16"

grub_default=/etc/default/grub
[[ -f "${grub_default}" ]] \
  || die "base image has no ${grub_default}; refusing to invent a bootloader config"
grub_theme="${grub_dir}/theme.txt"
grub_line="GRUB_THEME=\"${grub_theme}\""
if grep -q '^GRUB_THEME=' "${grub_default}"; then
  tmp=$(mktemp)
  sed "s|^GRUB_THEME=.*|${grub_line}|" "${grub_default}" > "${tmp}"
  cat "${tmp}" > "${grub_default}"
  rm -f "${tmp}"
else
  printf '\n%s\n' "${grub_line}" >> "${grub_default}"
fi
grep -q '^GRUB_THEME="/usr/share/grub/themes/darkjuander/theme.txt"$' "${grub_default}" \
  || die "failed to set GRUB_THEME in ${grub_default}"

bg_dir=/usr/share/backgrounds/darkjuander
need "${bg_dir}/darkjuander-3840x2160.png"
need "${bg_dir}/darkjuander-2560x1600.png"
need "${bg_dir}/darkjuander-1920x1080.png"
ln -sfn darkjuander-3840x2160.png "${bg_dir}/default.png"
ln -sfn darkjuander/default.png /usr/share/backgrounds/default.png
[[ -s /usr/share/backgrounds/default.png ]] \
  || die "default background symlink is broken"
need /usr/share/gnome-background-properties/darkjuander.xml
grep -q '/usr/share/backgrounds/darkjuander/darkjuander-3840x2160.png' \
  /usr/share/gnome-background-properties/darkjuander.xml \
  || die "background metadata does not point at the installed wallpaper"

need /usr/share/icons/hicolor/scalable/apps/darkjuander.svg
need /usr/share/icons/hicolor/16x16/apps/darkjuander.png
need /usr/share/icons/hicolor/48x48/apps/darkjuander.png
need /usr/share/icons/hicolor/256x256/apps/darkjuander.png

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
wanted = {
    "NAME": '"DARKJUANDER"',
    "PRETTY_NAME": '"DARKJUANDER"',
    "LOGO": "darkjuander",
    "HOME_URL": '"https://github.com/Awkward8787/DARKJUANDER"',
    "ID_LIKE": "fedora",
}
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
got = {}
for line in text.splitlines():
    if not line or line.startswith("#") or "=" not in line:
        continue
    key, value = line.split("=", 1)
    got[key] = value
for key, value in wanted.items():
    if got.get(key) != value:
        sys.exit(f"error: {path} has {key}={got.get(key)!r}, expected {value}")
if "ID=" not in got:
    sys.exit(f"error: {path} lost ID=")
print("os-release updated: NAME, PRETTY_NAME, LOGO, HOME_URL, ID_LIKE=fedora")
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
grep -q '^ID_LIKE=fedora$' "${os_release}" || die "ID_LIKE=fedora was not written"

# Login greeting from the brand guide. The status motd is marked planned
# there and is not installed.
cat > /etc/issue <<'EOF'
DARKJUANDER
Welcome back. Your agents are where you left them.

EOF
grep -q 'Welcome back. Your agents are where you left them.' /etc/issue \
  || die "failed to write /etc/issue"
if [[ -f /etc/motd ]] && grep -q 'umbrad: running' /etc/motd; then
  die "refusing to leave a hard-coded motd; the brand guide marks that line planned"
fi

dnf -y remove --setopt=clean_requirements_on_remove=False python3-fonttools
dnf -y clean all

echo "DARKJUANDER branding files are in the image. Plymouth, GRUB, wallpaper, icons, terminal colors, os-release, and /etc/issue are set. This has not been booted."
