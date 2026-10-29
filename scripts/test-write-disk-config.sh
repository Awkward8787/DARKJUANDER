#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "${tmp}"' EXIT

python3 "${root}/scripts/write-disk-config.py" qcow2 darkjuander \
  "console=ttyS0" "systemd.mask=plymouth-start.service" "${tmp}/qcow2.toml"
grep -q 'name = "dj"' "${tmp}/qcow2.toml"
grep -q 'password = "darkjuander"' "${tmp}/qcow2.toml"
if grep -q 'installer.kickstart' "${tmp}/qcow2.toml"; then
  echo "qcow2 config must not carry the ISO kickstart" >&2
  exit 1
fi

python3 "${root}/scripts/write-disk-config.py" anaconda-iso darkjuander \
  "console=ttyAMA0" "systemd.mask=plymouth-start.service" "${tmp}/iso.toml"
grep -q 'installer.kickstart' "${tmp}/iso.toml"
grep -q 'user --name=dj' "${tmp}/iso.toml"
grep -q -- '--iscrypted' "${tmp}/iso.toml"
grep -q 'fedora-darkjuander.repo' "${tmp}/iso.toml"
grep -q 'enabled' "${tmp}/iso.toml"
if grep -q 'password = "darkjuander"' "${tmp}/iso.toml"; then
  echo "ISO kickstart must not embed the plaintext password" >&2
  exit 1
fi
if grep -q '^\[\[customizations.user\]\]' "${tmp}/iso.toml"; then
  echo "ISO config must not combine customizations.user with kickstart" >&2
  exit 1
fi

echo "write-disk-config: ok"
