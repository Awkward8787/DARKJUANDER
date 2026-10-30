#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
script=${root}/scripts/assert-index-children.py
tmp=$(mktemp -d)
trap 'rm -rf "${tmp}"' EXIT

printf 'sha256:%s\n' "$(printf 'a%.0s' {1..64})" > "${tmp}/aarch64.digest"
printf 'sha256:%s\n' "$(printf 'b%.0s' {1..64})" > "${tmp}/x86_64.digest"
arm=$(tr -d '\n' < "${tmp}/aarch64.digest")
amd=$(tr -d '\n' < "${tmp}/x86_64.digest")

cat > "${tmp}/ok.json" << EOF
{"manifests":[
  {"digest":"${arm}","platform":{"architecture":"arm64","os":"linux"}},
  {"digest":"${amd}","platform":{"architecture":"amd64","os":"linux"}}
]}
EOF
python3 "${script}" "${tmp}/ok.json" "${tmp}/aarch64.digest" "${tmp}/x86_64.digest"

cat > "${tmp}/swapped.json" << EOF
{"manifests":[
  {"digest":"${amd}","platform":{"architecture":"arm64","os":"linux"}},
  {"digest":"${arm}","platform":{"architecture":"amd64","os":"linux"}}
]}
EOF
if python3 "${script}" "${tmp}/swapped.json" "${tmp}/aarch64.digest" "${tmp}/x86_64.digest" >/dev/null 2>&1; then
  echo "swapped children should fail" >&2
  exit 1
fi

cat > "${tmp}/one.json" << EOF
{"manifests":[
  {"digest":"${amd}","platform":{"architecture":"amd64","os":"linux"}}
]}
EOF
if python3 "${script}" "${tmp}/one.json" "${tmp}/aarch64.digest" "${tmp}/x86_64.digest" >/dev/null 2>&1; then
  echo "a single-arch index should fail" >&2
  exit 1
fi

printf '{not json\n' > "${tmp}/bad.json"
rc=0
err=$(python3 "${script}" "${tmp}/bad.json" "${tmp}/aarch64.digest" "${tmp}/x86_64.digest" 2>&1) || rc=$?
if [[ "${rc}" -eq 0 || "${err}" == *"Traceback"* || "${err}" != *"not JSON"* ]]; then
  echo "bad JSON should fail without a traceback" >&2
  exit 1
fi

cat > "${tmp}/windows.json" << EOF
{"manifests":[
  {"digest":"${arm}","platform":{"architecture":"arm64","os":"windows"}},
  {"digest":"${amd}","platform":{"architecture":"amd64","os":"linux"}}
]}
EOF
if python3 "${script}" "${tmp}/windows.json" "${tmp}/aarch64.digest" "${tmp}/x86_64.digest" >/dev/null 2>&1; then
  echo "a non-linux child should fail" >&2
  exit 1
fi

echo "assert-index-children: ok"
