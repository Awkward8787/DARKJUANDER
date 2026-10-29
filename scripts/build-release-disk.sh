#!/usr/bin/env bash
# Build one disk image from a local rootful podman image.
# bootc-image-builder digest resolved from quay tag latest on 2026-10-29.
# Usage: build-release-disk.sh IMAGE_TAG OUTPUT_DIR qcow2|anaconda-iso
#
# qcow2 uses IMAGE_TAG as shipped. anaconda-iso builds a throwaway tag with
# Fedora's dnf repos enabled, because the installer depsolve reads the
# container it is given. That tag is not the image pushed to GHCR.
set -euo pipefail

if [[ $# -lt 3 ]]; then
  echo "usage: build-release-disk.sh IMAGE_TAG OUTPUT_DIR qcow2|anaconda-iso" >&2
  exit 1
fi

tag=$1
image_type=$3
case "${image_type}" in
  qcow2|anaconda-iso) ;;
  *)
    echo "unsupported image type ${image_type}" >&2
    exit 1
    ;;
esac

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
work=$(mktemp -d)
trap 'rm -rf "${cfg}" "${work}"' EXIT
# aarch64 virt (QEMU and UTM) exposes the serial port as ttyAMA0. ttyS0 is
# the x86 UART. The last console= is /dev/console, which is where the login
# prompt and "Reached target multi-user" are written. tty0 first hides both
# from the serial log. NetworkManager-wait-online and plymouth block
# multi-user when the VM has no graphics and no DHCP lease.
case "$(uname -m)" in
  aarch64|arm64) console="console=ttyAMA0,115200 earlycon" ;;
  *) console="console=tty0 console=ttyS0,115200" ;;
esac
# Fedora's device job timeout is 45s. Under TCG, udev is still enumerating
# /boot and ttyAMA0 at that mark, local-fs fails, and the VM drops to
# emergency mode with a locked root shell. A longer timeout does not delay
# a fast boot; it only lets a slow one finish.
boot_units="systemd.default_device_timeout_sec=300 systemd.default_timeout_start_sec=300 systemd.mask=NetworkManager-wait-online.service systemd.mask=plymouth-quit-wait.service systemd.mask=plymouth-start.service"
cat > "${cfg}" << EOF
[[customizations.user]]
name = "dj"
password = "${password}"
groups = ["wheel"]

[customizations.kernel]
append = "${console} ${boot_units}"
EOF

source_tag=${tag}
if [[ "${image_type}" == "anaconda-iso" ]]; then
  source_tag="${tag}-iso-depsolve"
  # Throwaway layer only. scripts/iso-depsolve.sh writes an enabled repo
  # when the image has none, and points gpgkey at an ASCII-armored key
  # already in the image. This tag is not pushed to GHCR and is not the qcow2.
  cp "${root}/scripts/iso-depsolve.sh" "${work}/iso-depsolve.sh"
  cat > "${work}/Containerfile" << EOF
FROM ${tag}
COPY iso-depsolve.sh /tmp/iso-depsolve.sh
RUN /tmp/iso-depsolve.sh && rm -f /tmp/iso-depsolve.sh
EOF
  sudo podman build -f "${work}/Containerfile" -t "${source_tag}" "${work}"
fi

type_out="${out}/${image_type}"
mkdir -p "${type_out}"
echo "bootc-image-builder ${image_type} from ${source_tag} into ${type_out}"
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
  "${source_tag}"

sudo chown -R "$(id -u):$(id -g)" "${type_out}"
if [[ "${image_type}" == "anaconda-iso" ]]; then
  sudo podman rmi "${source_tag}" >/dev/null 2>&1 || true
  mapfile -t disks < <(find "${type_out}" -type f -name '*.iso')
else
  mapfile -t disks < <(find "${type_out}" -type f -name '*.qcow2')
fi
if [[ ${#disks[@]} -eq 0 ]]; then
  echo "bootc-image-builder wrote no ${image_type} under ${type_out}" >&2
  find "${type_out}" -print >&2
  exit 1
fi
printf '%s\n' "${disks[@]}"
