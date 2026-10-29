#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
script=${root}/scripts/assert-cosign-docker-config.sh
tmp=$(mktemp -d)
trap 'rm -rf "${tmp}"' EXIT

write_case() {
  local name=$1 body=$2 expect=$3
  local dir=${tmp}/${name}
  mkdir -p "${dir}"
  printf '%s\n' "${body}" > "${dir}/config.json"
  local rc=0
  DOCKER_CONFIG="${dir}" "${script}" >/dev/null 2>&1 || rc=$?
  if [[ "${expect}" == pass && "${rc}" -ne 0 ]]; then
    echo "${name} should pass" >&2
    exit 1
  fi
  if [[ "${expect}" == fail && "${rc}" -eq 0 ]]; then
    echo "${name} should fail" >&2
    exit 1
  fi
}

valid=$(printf 'user:token' | base64 -w 0)
user_only=$(printf 'user:' | base64 -w 0)
user_space=$(printf 'user: ' | base64 -w 0)
space_tok=$(printf ' :tok' | base64 -w 0)
write_case valid "{\"auths\":{\"ghcr.io\":{\"auth\":\"${valid}\"}}}" pass
write_case barex '{"auths":{"ghcr.io":{"auth":"x"}}}' fail
write_case emptycolon '{"auths":{"ghcr.io":{"auth":"Og=="}}}' fail
write_case space '{"auths":{"ghcr.io":{"auth":" "}}}' fail
write_case usercolon "{\"auths\":{\"ghcr.io\":{\"auth\":\"${user_only}\"}}}" fail
write_case userspace "{\"auths\":{\"ghcr.io\":{\"auth\":\"${user_space}\"}}}" fail
write_case spacetok "{\"auths\":{\"ghcr.io\":{\"auth\":\"${space_tok}\"}}}" fail
write_case store "{\"credsStore\":\"missing\",\"auths\":{\"ghcr.io\":{\"auth\":\"${valid}\"}}}" fail
write_case helper "{\"credHelpers\":{\"ghcr.io\":\"missing\"},\"auths\":{\"ghcr.io\":{\"auth\":\"${valid}\"}}}" fail

echo "assert-cosign-docker-config: ok"
