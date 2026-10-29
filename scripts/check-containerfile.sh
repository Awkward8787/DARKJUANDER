#!/usr/bin/env bash
# Static checks for image/Containerfile. This does not build the image.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
file=${root}/image/Containerfile
if [[ ! -f "${file}" ]]; then
  echo "missing ${file}" >&2
  exit 1
fi

grep -q 'quay.io/fedora/fedora-bootc:45@sha256:ed6ebf922c4c63704b1ff7befff74c8d06a5fb74a37b817df3d2cd093568bbc5' "${file}"
grep -q 'umbrad.service' "${file}"
grep -q 'nftables.service' "${file}"
grep -q 'plymouth-plugin-script' "${file}"
grep -q 'plymouth-plugin-label' "${file}"
grep -q 'generic-logos' "${file}"
grep -q 'python3 /tmp/darkjuander-branding/scripts/build.py --only os' "${file}"
grep -q 'dracut --force' "${file}"
grep -q 'check-initramfs.sh' "${file}"
grep -q 'UseSimpledrm=1' "${file}"
grep -q '50-darkjuander-inter.conf' "${file}"
initramfs=${root}/image/check-initramfs.sh
grep -q 'usr/share/plymouth/themes/darkjuander/darkjuander.script' "${initramfs}"
grep -q 'lsinitrd -f etc/plymouth/plymouthd.conf' "${initramfs}"
grep -q 'Theme=darkjuander' "${initramfs}"
grep -q 'Plymouth.ttf -> .*Inter' "${initramfs}"
grep -q '140 \* 1024 \* 1024' "${initramfs}"
if grep -q '60-darkjuander-inter.conf' "${file}"; then
  echo "fontconfig conf must sort before Noto (50- or 55-, not 60-)." >&2
  exit 1
fi
grep -q '10-darkjuander.toml' "${file}"
grep -q 'bash /tmp/apply-branding.sh' "${file}"
grep -q 'LicenseRef-DARKJUANDER-Brand' "${file}"
if grep -q 'grub2-tools-extra' "${file}"; then
  echo "Containerfile must not install a GRUB theme toolchain. bootupd does not load it." >&2
  exit 1
fi
if grep -Eqi 'secure boot is enabled|image is signed|booted in utm' "${file}"; then
  echo "Containerfile must not claim a boot or a signature that this repo has not produced." >&2
  exit 1
fi

brand=${root}/image/apply-branding.sh
if [[ ! -f "${brand}" ]]; then
  echo "missing ${brand}" >&2
  exit 1
fi
grep -q 'plymouth-set-default-theme darkjuander' "${brand}"
grep -q 'plymouth-plugin-script' "${brand}"
grep -q 'plymouth-plugin-label' "${brand}"
grep -q 'NAME="DARKJUANDER"' "${brand}"
grep -q 'PRETTY_NAME="DARKJUANDER"' "${brand}"
grep -q 'LOGO=darkjuander' "${brand}"
grep -q 'HOME_URL="https://github.com/Awkward8787/DARKJUANDER"' "${brand}"
grep -q 'ID=fedora' "${brand}"
grep -q 'generic-logos' "${brand}"
grep -q 'issue.d repeats DARKJUANDER' "${brand}"
if grep -q 'grub2-mkfont' "${brand}"; then
  echo "apply-branding.sh must not build a GRUB theme." >&2
  exit 1
fi
if grep -q 'ID_LIKE=fedora' "${brand}"; then
  echo "apply-branding.sh must not set ID_LIKE while ID stays fedora." >&2
  exit 1
fi
if grep -Eq '\|\| true' "${file}" "${brand}"; then
  echo "image branding must fail when an asset is missing, not ignore the error" >&2
  exit 1
fi
if grep -Eqi 'secure boot is enabled|image is signed|booted in utm' "${brand}"; then
  echo "apply-branding.sh must not claim a boot or a signature." >&2
  exit 1
fi
echo "Containerfile references Fedora bootc 45, umbrad, nftables, Plymouth, and dracut."
