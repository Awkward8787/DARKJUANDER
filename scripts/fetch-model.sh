#!/usr/bin/env bash
# Download a GGUF and refuse it unless the SHA-256 matches.
# Defaults to the SmolLM2 135M Q4_K_M file published by ggml-org.
# The checksum below was recomputed in this repo's development environment
# on 2026-10-28 against that URL. The script checks it again on every download.
set -euo pipefail

url=${1:-https://huggingface.co/ggml-org/SmolLM2-135M-GGUF/resolve/1076b85d135017971845b19b8d07df1479155988/SmolLM2-135M-Q4_K_M.gguf}
expected=${2:-e3131339bf4e8065265593d4fd8f7bb7ff2d3abff1edb5618aa1197b89cad9f5}
dest=${3:-${HOME}/.local/share/darkjuander/models/SmolLM2-135M-Q4_K_M.gguf}

mkdir -p "$(dirname "${dest}")"
tmp=${dest}.partial
curl -fL --retry 3 -o "${tmp}" "${url}"
got=$(sha256sum "${tmp}" | awk '{print $1}')
if [[ "${got}" != "${expected}" ]]; then
  rm -f "${tmp}"
  echo "checksum mismatch: got ${got}, expected ${expected}" >&2
  exit 1
fi
mv "${tmp}" "${dest}"
echo "verified ${dest}"
echo "license: Apache-2.0 (SmolLM2 / ggml-org GGUF). This script does not copy the file into git."
