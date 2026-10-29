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
cat > "${cfg}" << EOF
[[customizations.user]]
name = "dj"
password = "${password}"
groups = ["wheel"]

[customizations.kernel]
append = "console=tty0 console=ttyS0,115200"
EOF

source_tag=${tag}
if [[ "${image_type}" == "anaconda-iso" ]]; then
  source_tag="${tag}-iso-depsolve"
  # Fedora bootc keeps keys under /usr/share/pki/rpm-gpg. The shipped repo
  # files point at /etc/pki/rpm-gpg/RPM-GPG-KEY-fedora-45-$basearch, which is
  # missing. Do not download fedora.gpg: that file is binary OpenPGP and DNF
  # fails decoding it as UTF-8. Point gpgkey at the share keys that already
  # exist in the image.
  cat > "${work}/Containerfile" << EOF
FROM ${tag}
# Throwaway layer for Anaconda depsolve only. Not the GHCR or qcow2 image.
RUN set -eux; \\
    if ls /etc/yum.repos.d/*.repo >/dev/null 2>&1; then \\
      sed -i -E 's/^(enabled[[:space:]]*=[[:space:]]*)0/\\11/' /etc/yum.repos.d/*.repo; \\
    fi; \\
    keydir=/usr/share/pki/rpm-gpg; \\
    if ! ls "\${keydir}"/RPM-GPG-KEY-fedora-* >/dev/null 2>&1; then \\
      echo "no Fedora GPG keys under \${keydir}" >&2; \\
      ls -la /usr/share/pki/rpm-gpg /etc/pki/rpm-gpg 2>/dev/null || true; \\
      exit 1; \\
    fi; \\
    arch=\$(uname -m); \\
    case "\${arch}" in \\
      aarch64|arm64) key=RPM-GPG-KEY-fedora-45-aarch64 ;; \\
      x86_64|amd64) key=RPM-GPG-KEY-fedora-45-x86_64 ;; \\
      *) key=RPM-GPG-KEY-fedora-45-primary ;; \\
    esac; \\
    if [[ ! -f "\${keydir}/\${key}" ]]; then \\
      key=\$(basename "\$(ls "\${keydir}"/RPM-GPG-KEY-fedora-45-* | head -n 1)"); \\
    fi; \\
    sed -i -E "s|^gpgkey=.*|gpgkey=file://\${keydir}/\${key}|" /etc/yum.repos.d/*.repo; \\
    if ! grep -RqsE '^enabled[[:space:]]*=[[:space:]]*1' /etc/yum.repos.d; then \\
      printf '%s\\n' \\
        '[fedora]' \\
        'name=Fedora 45 Everything' \\
        'metalink=https://mirrors.fedoraproject.org/metalink?repo=fedora-45&arch=\$basearch' \\
        'enabled=1' \\
        'countme=0' \\
        'gpgcheck=1' \\
        "gpgkey=file://\${keydir}/\${key}" \\
        > /etc/yum.repos.d/fedora-darkjuander.repo; \\
    fi
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
