#!/usr/bin/env bash
# Turn the local dj-base image into raw and qcow2 disks with bootc-image-builder.
# This needs rootful privileged podman. It was not run in CI.
# Pass an SSH authorized_keys file. The script does not set a password.
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "usage: scripts/build-disk.sh AUTHORIZED_KEYS [image-tag]" >&2
  exit 1
fi

keys=$(realpath "$1")
tag=${2:-localhost/dj-base:0.1.0}
root=$(cd "$(dirname "$0")/.." && pwd)
out=${root}/out/disk
mkdir -p "${out}"
cp "${keys}" "${out}/authorized_keys"

if ! podman image exists "${tag}"; then
  echo "image ${tag} is not local. Run scripts/build-image.sh first." >&2
  exit 1
fi

# CentOS bootc-image-builder reads the image from the host's container storage
# when --local is set. The authorized_keys file is mounted into the builder.
podman run --rm \
  --privileged \
  --security-opt label=type:unconfined_t \
  -v "${out}:/output" \
  -v /var/lib/containers/storage:/var/lib/containers/storage \
  quay.io/centos-bootc/bootc-image-builder:latest \
  --type raw \
  --type qcow2 \
  --local \
  --root-ssh-authorized-keys /output/authorized_keys \
  "${tag}"

echo "disks, if the builder succeeded, are under ${out}"
echo "inbound SSH is dropped by the guest nftables rules. Use the hypervisor console, or add an SSH accept rule before you rely on the key."
echo "this script does not enroll Secure Boot keys and does not seal LUKS to a TPM."
