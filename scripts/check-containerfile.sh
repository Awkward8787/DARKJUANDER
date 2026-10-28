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
grep -q 'plymouth-scripts' "${file}"
grep -q 'grub2-tools-extra' "${file}"
grep -q 'python3-fonttools' "${file}"
grep -q 'branding/plymouth/darkjuander' "${file}"
grep -q 'branding/grub/darkjuander' "${file}"
grep -q 'branding/wallpapers' "${file}"
grep -q 'branding/os-icon/hicolor' "${file}"
grep -q 'branding/terminal' "${file}"
grep -q 'branding/src/fonts' "${file}"
grep -q 'bash /tmp/apply-branding.sh' "${file}"
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
grep -q 'grub2-mkfont' "${brand}"
grep -q 'NAME="DARKJUANDER"' "${brand}"
grep -q 'PRETTY_NAME="DARKJUANDER"' "${brand}"
grep -q 'LOGO=darkjuander' "${brand}"
grep -q 'HOME_URL="https://github.com/Awkward8787/DARKJUANDER"' "${brand}"
grep -q 'ID_LIKE=fedora' "${brand}"
grep -q 'Welcome back. Your agents are where you left them.' "${brand}"
if grep -Eq '\|\| true' "${file}" "${brand}"; then
  echo "image branding must fail when an asset is missing, not ignore the error" >&2
  exit 1
fi
if grep -Eqi 'secure boot is enabled|image is signed|booted in utm' "${brand}"; then
  echo "apply-branding.sh must not claim a boot or a signature." >&2
  exit 1
fi
echo "Containerfile references Fedora bootc 45, umbrad, nftables, and Umbra Eclipse branding."
