#!/usr/bin/env bash
# The mirror script must not contact a registry. skopeo is a mock.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
script=${root}/scripts/mirror-fedora-bootc.sh
digest=$(tr -d '[:space:]' < "${root}/image/fedora-bootc.digest")
tmp=$(mktemp -d)
trap 'rm -rf "${tmp}"' EXIT

cat > "${tmp}/skopeo" << 'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >> "${SKOPEO_LOG}"
if [[ "$1" == "inspect" ]]; then
  if [[ "${SKOPEO_MODE}" == "present" ]]; then
    printf '%s\n' "${EXPECT_DIGEST}"
    exit 0
  fi
  if [[ "${SKOPEO_MODE}" == "copied" && -f "${SKOPEO_COPIED}" ]]; then
    printf '%s\n' "${EXPECT_DIGEST}"
    exit 0
  fi
  exit 1
fi
if [[ "$1" == "copy" ]]; then
  if [[ "${SKOPEO_MODE}" == "gone" ]]; then
    exit 1
  fi
  touch "${SKOPEO_COPIED}"
  exit 0
fi
exit 1
EOF
chmod 0755 "${tmp}/skopeo"

run_mode() {
  local mode=$1
  shift
  SKOPEO_MODE="${mode}" \
  SKOPEO_LOG="${tmp}/log" \
  SKOPEO_COPIED="${tmp}/copied" \
  EXPECT_DIGEST="${digest}" \
  PATH="${tmp}:${PATH}" \
  "$@"
}

rm -f "${tmp}/log" "${tmp}/copied"
run_mode present "${script}" >/dev/null
if grep -q '^copy ' "${tmp}/log"; then
  echo "an existing mirror must not be copied again" >&2
  exit 1
fi

rm -f "${tmp}/log" "${tmp}/copied"
run_mode copied "${script}" >/dev/null
grep -q -- "--all --preserve-digests" "${tmp}/log"
grep -q "docker://quay.io/fedora/fedora-bootc@${digest}" "${tmp}/log"
grep -q "docker://ghcr.io/awkward8787/fedora-bootc:45" "${tmp}/log"
if grep -q 'darkjuander' "${tmp}/log"; then
  echo "mirror copy must not name the release image" >&2
  exit 1
fi

rm -f "${tmp}/log" "${tmp}/copied"
rc=0
err=$(run_mode gone "${script}" 2>&1) || rc=$?
if [[ "${rc}" -eq 0 ]]; then
  echo "a missing pin on Quay and in the mirror must fail" >&2
  exit 1
fi
if [[ "${err}" != *"not on Quay"* ]]; then
  echo "missing pin must say it is not on Quay" >&2
  exit 1
fi

rm -f "${tmp}/log"
rc=0
FEDORA_BOOTC_MIRROR=ghcr.io/awkward8787/darkjuander \
  SKOPEO_MODE=present \
  SKOPEO_LOG="${tmp}/log" \
  SKOPEO_COPIED="${tmp}/copied" \
  EXPECT_DIGEST="${digest}" \
  PATH="${tmp}:${PATH}" \
  "${script}" >/dev/null 2>&1 || rc=$?
if [[ "${rc}" -eq 0 ]]; then
  echo "writing the release image name must fail" >&2
  exit 1
fi
if [[ -f "${tmp}/log" ]] && grep -q . "${tmp}/log"; then
  echo "the release image guard must run before skopeo" >&2
  exit 1
fi

echo "mirror-fedora-bootc: ok"
