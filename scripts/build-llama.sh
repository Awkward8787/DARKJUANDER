#!/usr/bin/env bash
# Build llama.cpp v0.8.0 (commit 8642541b255bfb5dc39cd351a178be09013a55d6) from source.
# CPU-only. The binary is not committed.
set -euo pipefail

tag=v0.8.0
commit=8642541b255bfb5dc39cd351a178be09013a55d6
dest=${1:-${HOME}/.local/src/llama.cpp}
prefix=${2:-${HOME}/.local}

rm -rf "${dest}"
git clone --depth 1 --branch "${tag}" https://github.com/ggml-org/llama.cpp.git "${dest}"
got=$(git -C "${dest}" rev-parse HEAD)
if [[ "${got}" != "${commit}" ]]; then
  echo "llama.cpp ${tag} resolved to ${got}, expected ${commit}" >&2
  exit 1
fi
cmake -S "${dest}" -B "${dest}/build" -DCMAKE_BUILD_TYPE=Release -DGGML_NATIVE=ON
cmake --build "${dest}/build" --target llama-server -j"$(nproc)"
install -D -m 0755 "${dest}/build/bin/llama-server" "${prefix}/bin/llama-server"
echo "installed ${prefix}/bin/llama-server"
echo "upstream license: MIT. Do not pass --tools or --agent; those run host tools."
echo "listen on 127.0.0.1:9931 (v0.8.0's default port) unless you change both this process and umbrad.yaml."
