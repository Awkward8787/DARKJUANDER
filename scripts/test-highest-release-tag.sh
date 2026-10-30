#!/usr/bin/env bash
# :latest moves only for the highest v* semver tag.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
script=${root}/scripts/highest-release-tag.py

check() {
  local name=$1 version=$2 expect=$3
  shift 3
  local rc=0
  printf '%s\n' "$@" | python3 "${script}" "${version}" >/dev/null 2>&1 || rc=$?
  if [[ "${expect}" == yes && "${rc}" -ne 0 ]]; then
    echo "${name} should be the highest" >&2
    exit 1
  fi
  if [[ "${expect}" == no && "${rc}" -eq 0 ]]; then
    echo "${name} should not move :latest" >&2
    exit 1
  fi
}

check only v0.1.0 yes "refs/tags/v0.1.0"
check newer v0.2.0 yes "refs/tags/v0.1.0" "refs/tags/v0.2.0"
check older v0.1.0 no "aaa refs/tags/v0.1.0" "bbb refs/tags/v0.2.0"
check prerelease v1.0.0 yes "refs/tags/v1.0.0-rc.1" "refs/tags/v1.0.0"
check not_prerelease v1.0.0-rc.1 no "refs/tags/v1.0.0-rc.1" "refs/tags/v1.0.0"
check pre_only v1.0.0-rc.1 no "refs/tags/v1.0.0-rc.1"
check higher_pre_ignored v1.0.0 yes "refs/tags/v9.0.0-rc.1" "refs/tags/v1.0.0"
check ten v1.10.0 yes "refs/tags/v1.9.0"
check nine v1.9.0 no "refs/tags/v1.10.0"
check leading_zero v1.02.0 yes "refs/tags/v1.2.0"
check peel v0.2.0 yes "refs/tags/v0.2.0" "refs/tags/v0.2.0^{}" "refs/tags/v0.1.0"
check ignore v0.1.0 yes "refs/tags/not-a-version" "refs/tags/v0.1.0"

echo "highest-release-tag: ok"
