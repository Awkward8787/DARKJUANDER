#!/usr/bin/env bash
# Static checks for image/Containerfile. This does not build the image.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
file=${root}/image/Containerfile
if [[ ! -f "${file}" ]]; then
  echo "missing ${file}" >&2
  exit 1
fi

grep -q 'quay.io/fedora/fedora-bootc:45@sha256:0688bb4fec1318fc2fce52497dd45f84f807f5ea9af1919569801dd05fd6241b' "${file}"
grep -q 'umbrad.service' "${file}"
grep -q 'nftables.service' "${file}"
if grep -Eqi 'secure boot is enabled|image is signed|booted in utm' "${file}"; then
  echo "Containerfile must not claim a boot or a signature that this repo has not produced." >&2
  exit 1
fi
echo "Containerfile references Fedora bootc 45, umbrad, and nftables."
