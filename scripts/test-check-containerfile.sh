#!/usr/bin/env bash
# The FROM pin is the instruction, not a comment that happens to contain the digest.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
check=${root}/scripts/check-containerfile.sh
digest=$(tr -d '[:space:]' < "${root}/image/fedora-bootc.digest")
tmp=$(mktemp -d)
trap 'rm -rf "${tmp}"' EXIT

printf 'FROM ghcr.io/awkward8787/fedora-bootc:45 # %s\n' "${digest}" > "${tmp}/commented"
rc=0
"${check}" --from "${tmp}/commented" "${digest}" >/dev/null 2>&1 || rc=$?
if [[ "${rc}" -eq 0 ]]; then
  echo "a :45 FROM with the digest only in a comment must fail" >&2
  exit 1
fi

printf 'FROM ghcr.io/awkward8787/fedora-bootc@%s\n' "${digest}" > "${tmp}/pinned"
"${check}" --from "${tmp}/pinned" "${digest}"

"${check}" >/dev/null
echo "check-containerfile: ok"
