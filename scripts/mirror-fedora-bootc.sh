#!/usr/bin/env bash
# Check or copy the pinned Fedora bootc index.
#
# The default is a read-only check of ghcr.io/awkward8787/fedora-bootc@<digest>.
# --write is what a main push or a workflow_dispatch from main runs. It copies
# to the tag 45-<first 12 hex digits of the digest>. It does not delete or
# move any other tag, including an older :45.
#
# A registry host may include a port. The tag is only the colon after the
# last slash, so localhost:5000/org/fedora-bootc stays intact.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
mode="check"
case "${1:-}" in
  "" | --check) mode="check" ;;
  --write) mode="write" ;;
  *)
    echo "usage: mirror-fedora-bootc.sh [--check|--write]" >&2
    exit 2
    ;;
esac

digest=$(tr -d '[:space:]' < "${root}/image/fedora-bootc.digest")
if [[ ! "${digest}" =~ ^sha256:[0-9a-f]{64}$ ]]; then
  echo "fedora-bootc pin must be sha256 and 64 hex digits" >&2
  exit 1
fi

# Strip a digest suffix and a tag, and keep a host:port.
image_name() {
  local ref=$1 last tag
  ref=${ref#docker://}
  if [[ "${ref}" == *@sha256:* ]]; then
    ref=${ref%@sha256:*}
  fi
  if [[ "${ref}" == */* ]]; then
    last=${ref##*/}
    if [[ "${last}" == *:* ]]; then
      tag=${last##*:}
      ref=${ref%:"${tag}"}
    fi
  fi
  printf '%s\n' "${ref}"
}

missing_manifest() {
  grep -Eqi 'manifest unknown|MANIFEST_UNKNOWN|name unknown' <<<"${1}"
}

inspect_ref() {
  local ref=$1 err_file
  err_file=$(mktemp)
  set +e
  INSPECT_OUT=$(skopeo inspect --format '{{.Digest}}' "docker://${ref}" 2>"${err_file}")
  INSPECT_RC=$?
  set -e
  INSPECT_ERR=$(cat "${err_file}")
  rm -f "${err_file}"
  INSPECT_OUT=$(printf '%s' "${INSPECT_OUT}" | tr -d '[:space:]')
}

mirror=${FEDORA_BOOTC_MIRROR:-ghcr.io/awkward8787/fedora-bootc}
source_image=${FEDORA_BOOTC_SOURCE:-quay.io/fedora/fedora-bootc}
name=$(image_name "${mirror}")
source_name=$(image_name "${source_image}")
short=${digest#sha256:}
short=${short:0:12}
pin_tag="45-${short}"

case "${name}" in
  *darkjuander*)
    echo "refusing to use ${name}; the release image is not the base mirror" >&2
    exit 1
    ;;
esac

inspect_ref "${name}@${digest}"
if [[ "${INSPECT_RC}" -eq 0 && "${INSPECT_OUT}" != "${digest}" ]]; then
  echo "mirror unreachable or tampered: ${name}@${digest} returned ${INSPECT_OUT}" >&2
  printf '%s\n' "${INSPECT_ERR}" >&2
  exit 1
fi
if [[ "${INSPECT_RC}" -ne 0 ]] && ! missing_manifest "${INSPECT_ERR}"; then
  echo "mirror unreachable or tampered" >&2
  printf '%s\n' "${INSPECT_ERR}" >&2
  exit 1
fi
have_digest=0
if [[ "${INSPECT_RC}" -eq 0 ]]; then
  have_digest=1
fi

if [[ "${mode}" == "check" ]]; then
  if [[ "${have_digest}" -ne 1 ]]; then
    echo "pinned digest ${digest} is not in ${name}. This job does not copy it." >&2
    printf '%s\n' "${INSPECT_ERR}" >&2
    exit 1
  fi
  echo "mirror has ${name}@${digest}"
  exit 0
fi

copy_to_tag() {
  local src=$1 err_file copy_rc copy_err
  echo "copying ${src} to ${name}:${pin_tag}"
  err_file=$(mktemp)
  set +e
  skopeo copy --all --preserve-digests \
    "docker://${src}" \
    "docker://${name}:${pin_tag}" \
    2>"${err_file}"
  copy_rc=$?
  set -e
  copy_err=$(cat "${err_file}")
  rm -f "${err_file}"
  if [[ "${copy_rc}" -ne 0 ]]; then
    COPY_ERR=${copy_err}
    return "${copy_rc}"
  fi
  COPY_ERR=
  return 0
}

if [[ "${have_digest}" -eq 0 ]]; then
  if ! copy_to_tag "${source_name}@${digest}"; then
    if missing_manifest "${COPY_ERR}"; then
      echo "pinned digest ${digest} is not on Quay" >&2
    else
      echo "mirror unreachable or tampered" >&2
    fi
    printf '%s\n' "${COPY_ERR}" >&2
    exit 1
  fi
else
  inspect_ref "${name}:${pin_tag}"
  if [[ "${INSPECT_RC}" -eq 0 && "${INSPECT_OUT}" != "${digest}" ]]; then
    echo "mirror unreachable or tampered: ${name}:${pin_tag} is ${INSPECT_OUT}, not ${digest}. The tag was not moved." >&2
    printf '%s\n' "${INSPECT_ERR}" >&2
    exit 1
  fi
  if [[ "${INSPECT_RC}" -ne 0 ]] && ! missing_manifest "${INSPECT_ERR}"; then
    echo "mirror unreachable or tampered" >&2
    printf '%s\n' "${INSPECT_ERR}" >&2
    exit 1
  fi
  if [[ "${INSPECT_RC}" -ne 0 ]]; then
    if ! copy_to_tag "${name}@${digest}"; then
      echo "mirror unreachable or tampered" >&2
      printf '%s\n' "${COPY_ERR}" >&2
      exit 1
    fi
  fi
fi

inspect_ref "${name}:${pin_tag}"
if [[ "${INSPECT_RC}" -ne 0 || "${INSPECT_OUT}" != "${digest}" ]]; then
  echo "mirror tag ${name}:${pin_tag} is not the pinned digest ${digest}" >&2
  printf '%s\n' "${INSPECT_OUT}" >&2
  printf '%s\n' "${INSPECT_ERR}" >&2
  exit 1
fi

inspect_ref "${name}@${digest}"
if [[ "${INSPECT_RC}" -ne 0 || "${INSPECT_OUT}" != "${digest}" ]]; then
  echo "mirror digest ${name}@${digest} is not the pinned digest" >&2
  printf '%s\n' "${INSPECT_OUT}" >&2
  printf '%s\n' "${INSPECT_ERR}" >&2
  exit 1
fi
echo "mirrored ${name}@${digest} as ${pin_tag}"
