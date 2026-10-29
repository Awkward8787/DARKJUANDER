#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
script=${root}/scripts/release-version.sh

run() {
  local out status
  out=$(mktemp)
  set +e
  GITHUB_OUTPUT=${out} "$@" "${script}" >/dev/null
  status=$?
  set -e
  if [[ "${status}" -ne 0 ]]; then
    rm -f "${out}"
    return "${status}"
  fi
  cat "${out}"
  rm -f "${out}"
}

got=$(run env GITHUB_REF=refs/tags/v1.2.3 GITHUB_SHA=aaaaaaaaaaaa GITHUB_EVENT_NAME=push DRY_RUN=)
printf '%s\n' "${got}" | grep -qx 'version=v1.2.3'
printf '%s\n' "${got}" | grep -qx 'publish=true'

got=$(run env GITHUB_REF=refs/tags/v1.2.3-rc.1 GITHUB_SHA=aaaaaaaaaaaa GITHUB_EVENT_NAME=push DRY_RUN=)
printf '%s\n' "${got}" | grep -qx 'version=v1.2.3-rc.1'
printf '%s\n' "${got}" | grep -qx 'publish=true'

got=$(run env GITHUB_REF=refs/tags/v1.2.3 GITHUB_SHA=aaaaaaaaaaaa GITHUB_EVENT_NAME=workflow_dispatch DRY_RUN=true)
printf '%s\n' "${got}" | grep -qx 'publish=false'

got=$(run env GITHUB_REF=refs/tags/v1.2.3 GITHUB_SHA=aaaaaaaaaaaa GITHUB_EVENT_NAME=workflow_dispatch DRY_RUN=false)
printf '%s\n' "${got}" | grep -qx 'publish=true'

got=$(run env GITHUB_REF=refs/heads/main GITHUB_SHA=0123456789abcdef GITHUB_EVENT_NAME=pull_request DRY_RUN=)
printf '%s\n' "${got}" | grep -qx 'version=0.0.0-dry.0123456789ab'
printf '%s\n' "${got}" | grep -qx 'publish=false'

# The tag is literal; command substitution must not run.
# shellcheck disable=SC2016
if run env GITHUB_REF='refs/tags/v1.2.3$(id)' GITHUB_SHA=aaaaaaaaaaaa GITHUB_EVENT_NAME=push DRY_RUN=; then
  echo "metacharacter tag should be refused" >&2
  exit 1
fi

echo "release-version: ok"
