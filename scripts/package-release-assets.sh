#!/usr/bin/env bash
# Copy release files into OUT_DIR. Split any file larger than SPLIT_BYTES
# (default 1900 MiB, under GitHub's 2 GiB release-asset limit) and write
# SHA256SUMS for the files as shipped. Does not compress.
set -euo pipefail

if [[ $# -lt 2 ]]; then
  echo "usage: package-release-assets.sh OUT_DIR FILE [FILE...]" >&2
  exit 1
fi

out=$1
shift
mkdir -p "${out}"
split_bytes=${SPLIT_BYTES:-1992294400}

for file in "$@"; do
  if [[ ! -f "${file}" ]]; then
    echo "missing ${file}" >&2
    exit 1
  fi
  base=$(basename "${file}")
  size=$(stat -c%s "${file}")
  if (( size > split_bytes )); then
    split -b "${split_bytes}" -d -a 2 "${file}" "${out}/${base}.part"
  else
    cp -f "${file}" "${out}/${base}"
  fi
done

tmp=$(mktemp)
(
  cd "${out}"
  files=$(find . -maxdepth 1 -type f ! -name SHA256SUMS -printf '%P\n' | sort)
  if [[ -z "${files}" ]]; then
    echo "no release files in ${out}" >&2
    exit 1
  fi
  # Intentional word-splitting is avoided: one name per line.
  printf '%s\n' "${files}" | xargs -d '\n' sha256sum > "${tmp}"
)
mv "${tmp}" "${out}/SHA256SUMS"
echo "wrote ${out}/SHA256SUMS"
