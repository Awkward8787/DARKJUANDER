#!/usr/bin/env bash
# The probe must create the digest file's directory before podman push.
# This uses stand-in podman and cosign binaries and does not contact a registry.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
script=${root}/scripts/cosign-ci-probe.sh
tmp=$(mktemp -d)
trap 'rm -rf "${tmp}"' EXIT

cat > "${tmp}/sudo" << 'EOF'
#!/bin/sh
exec "$@"
EOF
cat > "${tmp}/podman" << 'EOF'
#!/bin/sh
case "$1" in
  login)
    cat >/dev/null
    exit 0
    ;;
  build)
    exit 0
    ;;
  push)
    prev=
    digestfile=
    for arg in "$@"; do
      if [ "$prev" = "--digestfile" ]; then
        digestfile=$arg
      fi
      prev=$arg
    done
    if [ -z "$digestfile" ]; then
      echo "podman push missing --digestfile" >&2
      exit 1
    fi
    dir=$(dirname "$digestfile")
    if [ ! -d "$dir" ]; then
      echo "open ${digestfile}: no such file or directory" >&2
      exit 1
    fi
    printf '%s\n' 'sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb' > "$digestfile"
    exit 0
    ;;
  *)
    echo "unexpected podman $*" >&2
    exit 99
    ;;
esac
EOF
cat > "${tmp}/cosign" << 'EOF'
#!/bin/sh
case "$1" in
  login)
    cat >/dev/null
    dir=${DOCKER_CONFIG:-$HOME/.docker}
    mkdir -p "$dir"
    printf '%s\n' '{"auths":{"ghcr.io":{"auth":"dGVzdDp0b2tlbg=="}}}' > "$dir/config.json"
    exit 0
    ;;
  sign)
    printf '%s\n' "$*" >> "${COSIGN_SIGN_LOG:?}"
    exit 0
    ;;
  verify)
    exit 0
    ;;
  *)
    echo "unexpected cosign $*" >&2
    exit 99
    ;;
esac
EOF
chmod +x "${tmp}/sudo" "${tmp}/podman" "${tmp}/cosign"

sign_log=${tmp}/sign.log
: > "${sign_log}"
PATH="${tmp}:${PATH}" \
  DOCKER_CONFIG="${tmp}/docker" \
  COSIGN_SIGN_LOG="${sign_log}" \
  GH_ACTOR=tester \
  GH_TOKEN=token \
  GITHUB_REF=refs/heads/main \
  PROBE_REF=ghcr.io/awkward8787/darkjuander-ci-probe:test \
  "${script}"

grep -q 'darkjuander-ci-probe@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb' "${sign_log}"
if grep -q 'darkjuander:latest' "${sign_log}" || grep -q ':v[0-9]' "${sign_log}"; then
  echo "probe signed a release ref" >&2
  exit 1
fi
echo "cosign-ci-probe: ok"
