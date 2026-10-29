#!/usr/bin/env bash
# Exercise split and checksums without building an image.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "${tmp}"' EXIT

dd if=/dev/zero bs=10 count=1 of="${tmp}/small.bin" status=none
dd if=/dev/zero bs=3000 count=1 of="${tmp}/big.bin" status=none

SPLIT_BYTES=1000 "${root}/scripts/package-release-assets.sh" \
  "${tmp}/out" "${tmp}/small.bin" "${tmp}/big.bin"

test -f "${tmp}/out/small.bin"
test -f "${tmp}/out/big.bin.part00"
test -f "${tmp}/out/big.bin.part01"
test -f "${tmp}/out/big.bin.part02"
test ! -f "${tmp}/out/big.bin"

parts=$(stat -c%s "${tmp}/out/big.bin.part00")
test "${parts}" -eq 1000

(
  cd "${tmp}/out"
  sha256sum -c SHA256SUMS
)

cat "${tmp}/out/big.bin.part00" "${tmp}/out/big.bin.part01" "${tmp}/out/big.bin.part02" > "${tmp}/joined"
cmp "${tmp}/big.bin" "${tmp}/joined"
echo "package-release-assets: ok"
