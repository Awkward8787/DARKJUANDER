#!/usr/bin/env bash
# The ISO depsolve helper must create a repo when Fedora bootc shipped none,
# and must refuse a binary GPG key.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
script=${root}/scripts/iso-depsolve.sh
arch=$(uname -m)
case "${arch}" in
  aarch64|arm64) keyname=RPM-GPG-KEY-fedora-45-aarch64 ;;
  x86_64|amd64) keyname=RPM-GPG-KEY-fedora-45-x86_64 ;;
  *) keyname=RPM-GPG-KEY-fedora-45-primary ;;
esac

tmp=$(mktemp -d)
trap 'rm -rf "${tmp}"' EXIT

mkdir -p "${tmp}/empty/usr/share/pki/rpm-gpg"
printf '%s\n' '-----BEGIN PGP PUBLIC KEY BLOCK-----' 'armored' \
  > "${tmp}/empty/usr/share/pki/rpm-gpg/${keyname}"
ROOT="${tmp}/empty" "${script}"
repo=${tmp}/empty/etc/yum.repos.d/fedora-darkjuander.repo
test -f "${repo}"
grep -q '^enabled=1$' "${repo}"
grep -q "gpgkey=file:///usr/share/pki/rpm-gpg/${keyname}$" "${repo}"
# The repo file must keep dnf's $basearch token unexpanded.
# shellcheck disable=SC2016
grep -q 'arch=$basearch$' "${repo}"

mkdir -p "${tmp}/present/usr/share/pki/rpm-gpg" "${tmp}/present/etc/yum.repos.d"
printf '%s\n' '-----BEGIN PGP PUBLIC KEY BLOCK-----' 'armored' \
  > "${tmp}/present/usr/share/pki/rpm-gpg/${keyname}"
cat > "${tmp}/present/etc/yum.repos.d/fedora.repo" << EOF
[fedora]
enabled=0
gpgkey=file:///etc/pki/rpm-gpg/missing
EOF
ROOT="${tmp}/present" "${script}"
test ! -f "${tmp}/present/etc/yum.repos.d/fedora-darkjuander.repo"
grep -q '^enabled=1$' "${tmp}/present/etc/yum.repos.d/fedora.repo"
grep -q "gpgkey=file:///usr/share/pki/rpm-gpg/${keyname}$" \
  "${tmp}/present/etc/yum.repos.d/fedora.repo"

mkdir -p "${tmp}/binary/usr/share/pki/rpm-gpg"
printf '\x99\x00binary\n' > "${tmp}/binary/usr/share/pki/rpm-gpg/${keyname}"
if ROOT="${tmp}/binary" "${script}"; then
  echo "binary GPG key should be refused" >&2
  exit 1
fi

mkdir -p "${tmp}/nokey/etc"
if ROOT="${tmp}/nokey" "${script}"; then
  echo "missing GPG key should fail" >&2
  exit 1
fi

echo "iso-depsolve: ok"
