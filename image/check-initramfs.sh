#!/usr/bin/env bash
# Confirm the rebuilt initramfs actually contains the DARKJUANDER Plymouth
# theme. A name match on "darkjuander" is not enough: the sysctl file and the
# font directory also contain that string. Nothing here has been booted.
set -euo pipefail

die() {
  echo "error: $*" >&2
  exit 1
}

kver=$(ls -1 /usr/lib/modules)
if [[ "$(printf '%s\n' "${kver}" | wc -l)" -ne 1 ]]; then
  die "expected one kernel under /usr/lib/modules, found: ${kver}"
fi
initrd="/usr/lib/modules/${kver}/initramfs.img"
[[ -s "${initrd}" ]] || die "missing initramfs ${initrd}"

max_bytes=$((140 * 1024 * 1024))
bytes=$(stat -c '%s' "${initrd}")
if ((bytes > max_bytes)); then
  die "initramfs is ${bytes} bytes, over 140 MiB (${max_bytes})"
fi

listing=$(mktemp)
trap 'rm -f "${listing}"' EXIT
lsinitrd "${initrd}" > "${listing}"

grep -q 'usr/share/plymouth/themes/darkjuander/darkjuander.script$' "${listing}" \
  || die "initramfs listing has no usr/share/plymouth/themes/darkjuander/darkjuander.script"
grep -q 'usr/share/fonts/Plymouth.ttf -> .*Inter' "${listing}" \
  || die "initramfs Plymouth.ttf does not point at Inter"

theme_conf=$(lsinitrd -f etc/plymouth/plymouthd.conf "${initrd}")
if ! printf '%s\n' "${theme_conf}" | grep -qx 'Theme=darkjuander'; then
  echo "--- etc/plymouth/plymouthd.conf from the initramfs ---" >&2
  printf '%s\n' "${theme_conf}" >&2
  die "initramfs plymouthd.conf does not set Theme=darkjuander"
fi

echo "initramfs ${bytes} bytes; theme script, Theme=darkjuander, and Plymouth.ttf -> Inter are present"
