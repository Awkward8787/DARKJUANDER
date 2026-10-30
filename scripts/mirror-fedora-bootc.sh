#!/usr/bin/env bash
# Copy the pinned Fedora bootc index into ghcr.io/awkward8787/fedora-bootc.
# The release image ghcr.io/awkward8787/darkjuander is a different name and
# is never a destination. Quay deletes old digests. When this mirror already
# has the pin, later builds do not need Quay.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
digest=$(tr -d '[:space:]' < "${root}/image/fedora-bootc.digest")
if [[ ! "${digest}" =~ ^sha256:[0-9a-f]{64}$ ]]; then
  echo "fedora-bootc pin must be sha256 and 64 hex digits" >&2
  exit 1
fi

mirror=${FEDORA_BOOTC_MIRROR:-ghcr.io/awkward8787/fedora-bootc}
source_image=${FEDORA_BOOTC_SOURCE:-quay.io/fedora/fedora-bootc}
mirror=${mirror%%@*}
mirror=${mirror%%:*}
case "${mirror}" in
  *darkjuander*)
    echo "refusing to write ${mirror}; the release image is not the base mirror" >&2
    exit 1
    ;;
esac

if skopeo inspect --format '{{.Digest}}' "docker://${mirror}@${digest}" >/dev/null 2>&1; then
  echo "GHCR mirror already has ${mirror}@${digest}"
  exit 0
fi

echo "copying ${source_image}@${digest} to ${mirror}:45"
if ! skopeo copy --all --preserve-digests \
  "docker://${source_image}@${digest}" \
  "docker://${mirror}:45"; then
  echo "pinned fedora-bootc digest ${digest} is not on Quay and not in ${mirror}." >&2
  echo "Quay has deleted a pinned digest twice. Copy this digest into the mirror while Quay still has it." >&2
  exit 1
fi

seen=$(skopeo inspect --format '{{.Digest}}' "docker://${mirror}:45")
if [[ "${seen}" != "${digest}" ]]; then
  echo "mirror tag ${mirror}:45 is ${seen}, not the pinned digest ${digest}" >&2
  exit 1
fi
by_digest=$(skopeo inspect --format '{{.Digest}}' "docker://${mirror}@${digest}")
if [[ "${by_digest}" != "${digest}" ]]; then
  echo "mirror digest ${by_digest} is not ${digest}" >&2
  exit 1
fi
echo "mirrored ${mirror}@${digest}"
