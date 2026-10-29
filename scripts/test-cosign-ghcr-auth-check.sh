#!/usr/bin/env bash
# The auth check must fail closed on GHCR's unauthenticated error and
# accept a manifest-unknown response from a throwaway digest.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
script=${root}/scripts/cosign-ghcr-auth-check.sh

if COSIGN_AUTH_FIXTURE=missing GH_ACTOR=tester GH_TOKEN=token "${script}"; then
  :
else
  echo "manifest-unknown fixture should pass" >&2
  exit 1
fi

if COSIGN_AUTH_FIXTURE=reject GH_ACTOR=tester GH_TOKEN=token "${script}"; then
  echo "unauthorized fixture should fail" >&2
  exit 1
fi

echo "cosign-ghcr-auth-check: ok"
