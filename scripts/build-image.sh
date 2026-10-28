#!/usr/bin/env bash
# Build the dj-base OCI image with podman for the machine's architecture.
# Requires Linux, rustc, and podman. Does not produce a disk image.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "${root}"

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "build-image.sh runs on Linux. On a Mac, run it inside a Linux aarch64 VM or podman machine." >&2
  exit 1
fi

arch=$(uname -m)
case "${arch}" in
  aarch64|arm64) platform=linux/arm64 ;;
  x86_64) platform=linux/amd64 ;;
  *)
    echo "unsupported architecture ${arch}" >&2
    exit 1
    ;;
esac

cargo build --workspace --release --locked
mkdir -p out/stage
install -m 0755 target/release/umbrad out/stage/umbrad
install -m 0755 target/release/dj out/stage/dj
install -m 0755 target/release/dj-auditd out/stage/dj-auditd

tag=${DJ_IMAGE_TAG:-localhost/dj-base:0.1.0}
podman build --platform "${platform}" -f image/Containerfile -t "${tag}" .
echo "built ${tag} for ${platform}"
echo "this script does not sign the image. cosign signing is planned and not done."
echo "next: scripts/build-disk.sh /path/to/authorized_keys"
