#!/usr/bin/env bash
# Prove cosign can authenticate to GHCR without pushing an image.
#
# cosign login writes the runner user's docker config. A later cosign
# registry call must use that config. This script logs in, computes a
# signature name locally (triangulate), then asks GHCR for a digest that
# does not exist. Manifest-unknown means the token was accepted. An
# unauthenticated response is a failure.
set -euo pipefail

actor=${GH_ACTOR:?}
token=${GH_TOKEN:?}
image=${COSIGN_IMAGE:-ghcr.io/awkward8787/darkjuander}

if [[ "${COSIGN_AUTH_FIXTURE:-}" == "reject" ]]; then
  sample='UNAUTHORIZED: unauthenticated: User cannot be authenticated with the token provided.'
elif [[ "${COSIGN_AUTH_FIXTURE:-}" == "missing" ]]; then
  sample='manifest unknown: sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
else
  sample=
fi

registry_auth_rejected() {
  printf '%s\n' "$1" | grep -Eqi 'UNAUTHORIZED|cannot be authenticated|unauthenticated'
}

if [[ -n "${sample}" ]]; then
  if registry_auth_rejected "${sample}"; then
    echo "fixture classified as an auth rejection" >&2
    exit 1
  fi
  echo "fixture classified as a registry response, not an auth rejection"
  exit 0
fi

printf '%s\n' "${token}" | cosign login ghcr.io --username "${actor}" --password-stdin

throwaway=$(printf 'a%.0s' {1..64})
ref="${image}@sha256:${throwaway}"
cosign triangulate "${ref}"

set +e
err=$(cosign verify \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  --certificate-identity-regexp '^https://github.com/Awkward8787/DARKJUANDER/\.github/workflows/release\.yml@' \
  "${ref}" 2>&1)
status=$?
set -e
printf '%s\n' "${err}"
if [[ "${status}" -eq 0 ]]; then
  echo "throwaway digest must not verify" >&2
  exit 1
fi
if registry_auth_rejected "${err}"; then
  echo "cosign registry auth was rejected" >&2
  exit 1
fi
echo "cosign reached GHCR with the docker config from cosign login"
