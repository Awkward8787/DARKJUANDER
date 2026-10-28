#!/usr/bin/env bash
# Compile the draft SELinux module when the toolchain exists.
# A missing checkmodule is reported and is not a successful compile.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
src=${root}/image/selinux/darkjuander.te

if ! command -v checkmodule >/dev/null 2>&1; then
  echo "checkmodule is not installed. The draft at ${src} was not compiled." >&2
  exit 2
fi

out=$(mktemp)
trap 'rm -f "${out}"' EXIT
checkmodule -M -m -o "${out}" "${src}"
echo "checkmodule accepted ${src}. The module is still not loaded."
