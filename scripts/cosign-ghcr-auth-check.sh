#!/usr/bin/env bash
# Log cosign into GHCR and require the credential in the docker config
# cosign reads. Then ask about a digest that does not exist.
#
# A public package returns the same manifest answer to an anonymous client,
# so that read is not the auth check. The auth check is the ghcr.io entry
# written by cosign login. The read only passes on MANIFEST_UNKNOWN or
# "no signatures found". Every other result fails.
set -euo pipefail

actor=${GH_ACTOR:?}
token=${GH_TOKEN:?}
image=${COSIGN_IMAGE:-ghcr.io/awkward8787/darkjuander}
root=$(cd "$(dirname "$0")" && pwd)

printf '%s\n' "${token}" | cosign login ghcr.io --username "${actor}" --password-stdin
"${root}/assert-cosign-docker-config.sh"

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
if printf '%s\n' "${err}" | grep -q 'MANIFEST_UNKNOWN'; then
  echo "GHCR reported MANIFEST_UNKNOWN for the throwaway digest"
  exit 0
fi
if printf '%s\n' "${err}" | grep -F -q 'no signatures found'; then
  echo "cosign reported no signatures found for the throwaway digest"
  exit 0
fi
echo "cosign verify did not return an allowed registry answer" >&2
exit 1
