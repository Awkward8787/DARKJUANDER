#!/usr/bin/env bash
# Run the production auth check against a mocked cosign binary.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
script=${root}/scripts/cosign-ghcr-auth-check.sh
tmp=$(mktemp -d)
trap 'rm -rf "${tmp}"' EXIT

cat > "${tmp}/cosign" << 'EOF'
#!/bin/sh
case "$1" in
  login)
    cat >/dev/null
    dir=${DOCKER_CONFIG:-$HOME/.docker}
    mkdir -p "$dir"
    if [ "${COSIGN_MOCK_WRITE_AUTH:-1}" = "1" ]; then
      printf '%s\n' '{"auths":{"ghcr.io":{"auth":"dGVzdDp0b2tlbg=="}}}' > "$dir/config.json"
    else
      printf '%s\n' '{"auths":{}}' > "$dir/config.json"
    fi
    exit 0
    ;;
  triangulate)
    echo "ghcr.io/awkward8787/darkjuander:sha256-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.sig"
    exit 0
    ;;
  verify)
    if [ -n "${COSIGN_MOCK_VERIFY_TEXT-}" ]; then
      printf '%s\n' "${COSIGN_MOCK_VERIFY_TEXT}"
    fi
    exit "${COSIGN_MOCK_VERIFY_STATUS:-10}"
    ;;
  *)
    echo "unexpected cosign invocation: $*" >&2
    exit 99
    ;;
esac
EOF
chmod +x "${tmp}/cosign"

run_case() {
  local name=$1 status=$2 text=$3 expect=$4 write=${5:-1}
  local out rc
  set +e
  out=$(
    DOCKER_CONFIG="${tmp}/docker-${name}" \
      PATH="${tmp}:${PATH}" \
      COSIGN_MOCK_WRITE_AUTH="${write}" \
      COSIGN_MOCK_VERIFY_STATUS="${status}" \
      COSIGN_MOCK_VERIFY_TEXT="${text}" \
      GH_ACTOR=tester \
      GH_TOKEN=token \
      "${script}" 2>&1
  )
  rc=$?
  set -e
  if [[ "${expect}" == pass && "${rc}" -ne 0 ]]; then
    echo "${name} should pass: ${out}" >&2
    exit 1
  fi
  if [[ "${expect}" == fail && "${rc}" -eq 0 ]]; then
    echo "${name} should fail: ${out}" >&2
    exit 1
  fi
}

run_case manifest 10 'MANIFEST_UNKNOWN: manifest unknown' pass
run_case nosig 10 'Error: no signatures found' pass
run_case denied403 1 'DENIED: permission_denied: The token provided does not match expected scopes.' fail
run_case bare401 1 'unexpected status code 401' fail
run_case bare403 1 'unexpected status code 403 Forbidden' fail
run_case dns 1 'dial tcp: lookup ghcr.io: no such host' fail
run_case tls 1 'x509: certificate signed by unknown authority' fail
run_case timeout 1 'context deadline exceeded' fail
run_case rekor 1 '503 Service Unavailable' fail
run_case empty 1 '' fail
run_case wrongreg 1 'ghcr.example.invalid: no such host' fail
run_case verified 0 'no signatures found' fail
run_case noconfig 10 'MANIFEST_UNKNOWN: manifest unknown' fail 0
run_case mixed_unauth 10 'UNAUTHORIZED: nope
MANIFEST_UNKNOWN: manifest unknown' fail
run_case mixed_denied 10 'DENIED: permission_denied
Error: no signatures found' fail
run_case substring 10 'NOT_MANIFEST_UNKNOWN_X' fail

echo "cosign-ghcr-auth-check: ok"
