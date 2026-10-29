#!/usr/bin/env bash
# Sign one scratch image in ghcr.io/awkward8787/darkjuander-ci-probe.
# Never tags darkjuander:latest or a v* release.
#
# The Fulcio identity is this workflow ref. On main that is
# refs/heads/main, which does not match the SHA256SUMS identity
# (.../release.yml@refs/tags/v) in docs/INSTALL.md.
#
# podman 4.9 writes --digestfile only after the push, and the parent
# directory must already exist. The digest is kept next to the build
# context, not in a repo dist/ directory.
set -euo pipefail

actor=${GH_ACTOR:?}
token=${GH_TOKEN:?}
ref=${PROBE_REF:?}
root=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d)
digest_file=${work}/probe.digest
mkdir -p "$(dirname "${digest_file}")"

printf 'probe\n' > "${work}/probe.txt"
printf '%s\n' 'FROM scratch' 'COPY probe.txt /probe.txt' > "${work}/Containerfile"
printf '%s\n' "${token}" | sudo podman login ghcr.io -u "${actor}" --password-stdin
sudo podman build -f "${work}/Containerfile" -t "${ref}" "${work}"
sudo podman push --digestfile "${digest_file}" "${ref}"
grep -Eq '^sha256:[0-9a-f]{64}$' "${digest_file}"
printf '%s\n' "${token}" | cosign login ghcr.io --username "${actor}" --password-stdin
"${root}/assert-cosign-docker-config.sh"
digest=$(tr -d '\r\n' < "${digest_file}")
name=${ref%:*}
identity="${GITHUB_SERVER_URL:-https://github.com}/${GITHUB_REPOSITORY:-Awkward8787/DARKJUANDER}/.github/workflows/release.yml@${GITHUB_REF:-refs/heads/main}"
cosign sign --yes "${name}@${digest}"
cosign verify \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  --certificate-identity "${identity}" \
  "${name}@${digest}"
echo "probe signed ${name}@${digest}"
echo "probe identity ${identity} is not the SHA256SUMS tag identity"
