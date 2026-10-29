#!/usr/bin/env bash
# Log cosign into GHCR and require the credential in the docker config
# cosign reads. Then ask about a digest that does not exist.
#
# A public package returns the same manifest answer to an anonymous client,
# so that read is not the auth check. The auth check is the ghcr.io
# user:token written by cosign login. The read passes only when the verify
# output has MANIFEST_UNKNOWN or "no signatures found" and has none of the
# deny patterns (auth, HTTP 401/403, TLS, DNS, or timeout).
set -euo pipefail

actor=${GH_ACTOR:?}
token=${GH_TOKEN:?}
image=ghcr.io/awkward8787/darkjuander
root=$(cd "$(dirname "$0")" && pwd)

printf '%s\n' "${token}" | cosign login ghcr.io --username "${actor}" --password-stdin
"${root}/assert-cosign-docker-config.sh"

throwaway=$(printf 'a%.0s' {1..64})
ref="${image}@sha256:${throwaway}"
cosign triangulate "${ref}"

set +e
err=$(cosign verify \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  --certificate-identity-regexp '^https://github.com/Awkward8787/DARKJUANDER/\.github/workflows/release\.yml@refs/tags/v' \
  "${ref}" 2>&1)
status=$?
set -e
printf '%s\n' "${err}"
if [[ "${status}" -eq 0 ]]; then
  echo "throwaway digest must not verify" >&2
  exit 1
fi
if printf '%s\n' "${err}" | grep -Eqi 'UNAUTHORIZED|DENIED|denied|401|403|x509|no such host|deadline'; then
  echo "cosign verify output matched a deny pattern" >&2
  exit 1
fi
if printf '%s\n' "${err}" | grep -Eq '(^|[^A-Za-z0-9_])MANIFEST_UNKNOWN([^A-Za-z0-9_]|$)'; then
  echo "GHCR reported MANIFEST_UNKNOWN for the throwaway digest"
  exit 0
fi
if printf '%s\n' "${err}" | grep -Eq '(^|[^A-Za-z0-9_])no signatures found([^A-Za-z0-9_]|$)'; then
  echo "cosign reported no signatures found for the throwaway digest"
  exit 0
fi
echo "cosign verify did not return an allowed registry answer" >&2
exit 1
