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

# A dispatch from main may pass FEDORA_BOOTC_DIGEST. Empty uses the file.
# Anything else must be sha256 and 64 hex digits, or this refuses to copy.
if [[ -n "${FEDORA_BOOTC_DIGEST:-}" ]]; then
  digest=$(printf '%s' "${FEDORA_BOOTC_DIGEST}" | tr -d '[:space:]')
else
  digest=$(tr -d '[:space:]' < "${root}/image/fedora-bootc.digest")
fi
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

# A dispatch digest must be a multi-arch Fedora 45 index. Anything else
# stops before a copy. The version label lives on the amd64 image config.
require_dispatch_index() {
  local ref=$1 err_file raw rc
  err_file=$(mktemp)
  set +e
  raw=$(skopeo inspect --raw "docker://${ref}" 2>"${err_file}")
  rc=$?
  set -e
  if [[ "${rc}" -ne 0 ]]; then
    echo "dispatch digest ${digest} cannot be inspected" >&2
    cat "${err_file}" >&2
    rm -f "${err_file}"
    exit 1
  fi
  rm -f "${err_file}"
  local child
  child=$(printf '%s' "${raw}" | python3 -c '
import json, sys
allowed = {
    "application/vnd.oci.image.index.v1+json",
    "application/vnd.docker.distribution.manifest.list.v2+json",
}
try:
    doc = json.load(sys.stdin)
except json.JSONDecodeError:
    sys.stderr.write("dispatch digest is not JSON\n")
    sys.exit(1)
if not isinstance(doc, dict) or doc.get("mediaType") not in allowed:
    sys.stderr.write("dispatch digest is not an OCI or Docker index\n")
    sys.exit(1)
manifests = doc.get("manifests")
if not isinstance(manifests, list):
    sys.stderr.write("dispatch digest has no manifest list\n")
    sys.exit(1)
child = ""
arches = set()
for item in manifests:
    if not isinstance(item, dict):
        continue
    plat = item.get("platform") or {}
    if not isinstance(plat, dict) or plat.get("os") != "linux":
        continue
    arch = plat.get("architecture")
    if arch in ("amd64", "arm64"):
        arches.add(arch)
        digest = item.get("digest")
        if arch == "amd64" and isinstance(digest, str):
            child = digest
if "amd64" not in arches or "arm64" not in arches or not child.startswith("sha256:"):
    sys.stderr.write("dispatch digest is missing an amd64 or arm64 child\n")
    sys.exit(1)
print(child)
') || {
    echo "dispatch digest ${digest} is not a Fedora 45 index" >&2
    exit 1
  }
  err_file=$(mktemp)
  set +e
  raw=$(skopeo inspect "docker://${source_name}@${child}" 2>"${err_file}")
  rc=$?
  set -e
  if [[ "${rc}" -ne 0 ]]; then
    echo "dispatch digest child cannot be inspected" >&2
    cat "${err_file}" >&2
    rm -f "${err_file}"
    exit 1
  fi
  rm -f "${err_file}"
  printf '%s' "${raw}" | python3 -c '
import json, sys
try:
    doc = json.load(sys.stdin)
except json.JSONDecodeError:
    sys.stderr.write("dispatch digest child is not JSON\n")
    sys.exit(1)
labels = {}
if isinstance(doc, dict):
    labels = doc.get("Labels") or doc.get("labels") or {}
if not isinstance(labels, dict):
    labels = {}
value = str(labels.get("org.opencontainers.image.version") or labels.get("version") or "")
if not value.startswith("45"):
    sys.stderr.write("dispatch digest version label does not start with 45\n")
    sys.exit(1)
' || {
    echo "dispatch digest ${digest} is not a Fedora 45 index" >&2
    exit 1
  }
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

if [[ "${mode}" == "write" && -n "${FEDORA_BOOTC_DIGEST:-}" ]]; then
  require_dispatch_index "${source_name}@${digest}"
fi

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
