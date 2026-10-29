#!/usr/bin/env bash
# Enable one Fedora dnf repository so bootc-image-builder can depsolve the
# Anaconda installer. Used only for the throwaway ISO image. The qcow2 and
# the GHCR image do not run this script.
#
# Fedora bootc 45 ships GPG keys under /usr/share/pki/rpm-gpg and does not
# ship /etc/yum.repos.d/*.repo. A sed of that unmatched glob fails the build
# before any repo file is written. fedora.gpg from fedoraproject.org is
# binary OpenPGP; DNF rejects it with a UTF-8 decode error. This script
# refuses a key that is not ASCII-armored.
#
# ROOT is empty when this runs in the container. Tests set ROOT to a scratch tree.
set -euo pipefail

root=${ROOT:-}
shopt -s nullglob

mkdir -p "${root}/etc/yum.repos.d"

keydir="${root}/usr/share/pki/rpm-gpg"
keys=("${keydir}"/RPM-GPG-KEY-fedora-*)
if [[ ${#keys[@]} -eq 0 ]]; then
  echo "no Fedora GPG keys under ${keydir}" >&2
  exit 1
fi

arch=$(uname -m)
case "${arch}" in
  aarch64|arm64) keyname=RPM-GPG-KEY-fedora-45-aarch64 ;;
  x86_64|amd64) keyname=RPM-GPG-KEY-fedora-45-x86_64 ;;
  *) keyname=RPM-GPG-KEY-fedora-45-primary ;;
esac
if [[ ! -f "${keydir}/${keyname}" ]]; then
  keyname=$(basename "${keys[0]}")
fi
keyfile="${keydir}/${keyname}"
if ! grep -I -q -- '^-----BEGIN PGP PUBLIC KEY BLOCK-----' "${keyfile}"; then
  echo "refusing GPG key that is not ASCII-armored: ${keyfile}" >&2
  exit 1
fi

# Repo file paths are inside the image, not the test scratch prefix.
gpgkey="file://${keyfile#"${root}"}"

repos=()
for dir in \
  "${root}/etc/yum.repos.d" \
  "${root}/usr/lib/yum.repos.d" \
  "${root}/etc/distro.repos.d"
do
  if [[ -d "${dir}" ]]; then
    for repo in "${dir}"/*.repo; do
      repos+=("${repo}")
    done
  fi
done

for repo in "${repos[@]}"; do
  sed -i -E 's/^(enabled[[:space:]]*=[[:space:]]*)0/\11/' "${repo}"
  if grep -q '^gpgkey=' "${repo}"; then
    sed -i -E "s|^gpgkey=.*|gpgkey=${gpgkey}|" "${repo}"
  fi
done

have_enabled=0
for repo in "${repos[@]}"; do
  if grep -qE '^enabled[[:space:]]*=[[:space:]]*1[[:space:]]*$' "${repo}"; then
    have_enabled=1
  fi
done

if [[ "${have_enabled}" -eq 0 ]]; then
  cat > "${root}/etc/yum.repos.d/fedora-darkjuander.repo" << EOF
[fedora]
name=Fedora 45 Everything
metalink=https://mirrors.fedoraproject.org/metalink?repo=fedora-45&arch=\$basearch
enabled=1
countme=0
gpgcheck=1
gpgkey=${gpgkey}
EOF
  echo "wrote ${root}/etc/yum.repos.d/fedora-darkjuander.repo (${gpgkey})"
else
  echo "enabled ${#repos[@]} existing repo file(s); gpgkey=${gpgkey}"
fi

search=()
for dir in \
  "${root}/etc/yum.repos.d" \
  "${root}/usr/lib/yum.repos.d" \
  "${root}/etc/distro.repos.d"
do
  if [[ -d "${dir}" ]]; then
    search+=("${dir}")
  fi
done
if [[ ${#search[@]} -eq 0 ]] || ! grep -RqsE '^enabled[[:space:]]*=[[:space:]]*1' "${search[@]}"; then
  echo "ISO depsolve image has no enabled dnf repository" >&2
  exit 1
fi
