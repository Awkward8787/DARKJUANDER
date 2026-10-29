#!/usr/bin/env bash
# Build a qcow2 and an Anaconda ISO from a local rootful podman image.
# bootc-image-builder digest resolved from quay tag latest on 2026-10-29.
# Usage: build-release-disk.sh IMAGE_TAG OUTPUT_DIR
set -euo pipefail

if [[ $# -lt 2 ]]; then
  echo "usage: build-release-disk.sh IMAGE_TAG OUTPUT_DIR" >&2
  exit 1
fi

tag=$1
mkdir -p "$2"
out=$(cd "$2" && pwd)
root=$(cd "$(dirname "$0")/.." && pwd)
bib=${BIB_IMAGE:-quay.io/centos-bootc/bootc-image-builder@sha256:b858892139e0779d48869a4dd8cbbc4e257da49f5a4b502824a55d221ed65a55}
password=$(grep -v '^[[:space:]]*#' "${root}/image/console-password.txt" | head -n 1 | tr -d '\r\n')
if [[ -z "${password}" ]]; then
  echo "console password file is empty" >&2
  exit 1
fi

cfg=$(mktemp)
trap 'rm -f "${cfg}"' EXIT
cat > "${cfg}" << EOF
[[customizations.user]]
name = "dj"
password = "${password}"
groups = ["wheel"]

[customizations.kernel]
append = "console=tty0 console=ttyS0,115200"
EOF

echo "bootc-image-builder output directory ${out}"
# One request cannot mix a disk image and an ISO.
for image_type in qcow2 anaconda-iso; do
  type_out="${out}/${image_type}"
  mkdir -p "${type_out}"
  sudo podman run --rm --privileged \
    --security-opt label=type:unconfined_t \
    --pull=newer \
    -v "${cfg}:/config.toml:ro" \
    -v "${type_out}:/output" \
    -v /var/lib/containers/storage:/var/lib/containers/storage \
    "${bib}" \
    --type "${image_type}" \
    --rootfs xfs \
    --local \
    --config /config.toml \
    "${tag}"
done

sudo chown -R "$(id -u):$(id -g)" "${out}"
mapfile -t qcows < <(find "${out}" -type f -name '*.qcow2')
mapfile -t isos < <(find "${out}" -type f -name '*.iso')
if [[ ${#qcows[@]} -eq 0 || ${#isos[@]} -eq 0 ]]; then
  echo "bootc-image-builder did not write both a qcow2 and an iso under ${out}" >&2
  find "${out}" -print >&2
  exit 1
fi
printf '%s\n' "${qcows[@]}" "${isos[@]}"
