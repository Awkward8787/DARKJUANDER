#!/usr/bin/env bash
# The mirror script must not contact a registry. skopeo is a mock.
# These cases fail if the script skips on a tag, or drops either post-check.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
script=${root}/scripts/mirror-fedora-bootc.sh
digest=$(tr -d '[:space:]' < "${root}/image/fedora-bootc.digest")
short=${digest#sha256:}
short=${short:0:12}
pin_tag="45-${short}"
wrong=$(printf 'sha256:%s' "$(printf 'b%.0s' {1..64})")
tmp=$(mktemp -d)
trap 'rm -rf "${tmp}"' EXIT

cat > "${tmp}/skopeo" << 'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >> "${SKOPEO_LOG}"
ref="${@: -1}"
if [[ "$1" == "copy" ]]; then
  if [[ "${COPY_MODE}" == "quay_missing" ]]; then
    echo "manifest unknown: QUAY-STDERR-MARKER" >&2
    exit 1
  fi
  touch "${SKOPEO_COPIED}"
  exit 0
fi
if [[ "$1" == "inspect" && "$*" == *"--raw"* ]]; then
  case "${RAW_MODE:-ok}" in
    single)
      echo '{"mediaType":"application/vnd.oci.image.index.v1+json","manifests":[{"digest":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","platform":{"architecture":"amd64","os":"linux"}}]}'
      ;;
    manifest)
      echo '{"mediaType":"application/vnd.oci.image.manifest.v1+json"}'
      ;;
    docker)
      echo '{"mediaType":"application/vnd.docker.distribution.manifest.list.v2+json","manifests":[{"digest":"sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","platform":{"architecture":"amd64","os":"linux"}},{"digest":"sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","platform":{"architecture":"arm64","os":"linux"}}]}'
      ;;
    *)
      echo '{"mediaType":"application/vnd.oci.image.index.v1+json","manifests":[{"digest":"sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","platform":{"architecture":"amd64","os":"linux"}},{"digest":"sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","platform":{"architecture":"arm64","os":"linux"}}]}'
      ;;
  esac
  exit 0
fi
if [[ "$1" == "inspect" && "$*" != *"--format"* ]]; then
  printf '{"Labels":{"org.opencontainers.image.version":"%s"}}\n' "${LABEL_VERSION:-45.1}"
  exit 0
fi
if [[ "${ref}" == *@sha256:* ]]; then
  if [[ -f "${SKOPEO_COPIED}" ]]; then
    if [[ "${DIGEST_AFTER}" == "wrong" ]]; then
      printf '%s\n' "${WRONG_DIGEST}"
      exit 0
    fi
    printf '%s\n' "${EXPECT_DIGEST}"
    exit 0
  fi
  if [[ "${DIGEST_BEFORE}" == "down" ]]; then
    echo "connection refused: MIRROR-STDERR-MARKER" >&2
    exit 1
  fi
  if [[ "${DIGEST_BEFORE}" == "ok" ]]; then
    printf '%s\n' "${EXPECT_DIGEST}"
    exit 0
  fi
  echo "manifest unknown: digest not in mirror" >&2
  exit 1
fi
if [[ "${TAG_BEFORE}" == "wrong" && ! -f "${SKOPEO_COPIED}" ]]; then
  printf '%s\n' "${WRONG_DIGEST}"
  exit 0
fi
if [[ "${TAG_BEFORE}" == "ok" || -f "${SKOPEO_COPIED}" ]]; then
  if [[ "${TAG_AFTER}" == "wrong" && -f "${SKOPEO_COPIED}" ]]; then
    printf '%s\n' "${WRONG_DIGEST}"
    exit 0
  fi
  printf '%s\n' "${EXPECT_DIGEST}"
  exit 0
fi
echo "manifest unknown: tag missing" >&2
exit 1
EOF
chmod 0755 "${tmp}/skopeo"

run_script() {
  DIGEST_BEFORE=${DIGEST_BEFORE:-missing} \
  DIGEST_AFTER=${DIGEST_AFTER:-ok} \
  TAG_BEFORE=${TAG_BEFORE:-missing} \
  TAG_AFTER=${TAG_AFTER:-ok} \
  COPY_MODE=${COPY_MODE:-ok} \
  SKOPEO_LOG="${tmp}/log" \
  SKOPEO_COPIED="${tmp}/copied" \
  EXPECT_DIGEST="${digest}" \
  WRONG_DIGEST="${wrong}" \
  PATH="${tmp}:${PATH}" \
  "$@"
}

reset() {
  rm -f "${tmp}/log" "${tmp}/copied"
  DIGEST_BEFORE=missing
  DIGEST_AFTER=ok
  TAG_BEFORE=missing
  TAG_AFTER=ok
  COPY_MODE=ok
}

reset
DIGEST_BEFORE=ok
TAG_BEFORE=ok
run_script "${script}" --write >/dev/null
if grep -q '^copy ' "${tmp}/log"; then
  echo "a mirror that already has the digest and the pin tag must not be copied" >&2
  exit 1
fi
if ! grep -q "@${digest}" "${tmp}/log"; then
  echo "the skip must inspect the digest, not only a tag" >&2
  exit 1
fi

# A tag that already resolves must not hide a missing digest.
reset
TAG_BEFORE=ok
run_script "${script}" --write >/dev/null
if ! grep -q "^copy --all --preserve-digests docker://quay.io/fedora/fedora-bootc@${digest} " "${tmp}/log"; then
  echo "a present tag must not skip the copy when the digest is missing" >&2
  exit 1
fi
# Only the copy destination counts. An inspect of the pin tag must not hide a copy to :45.
copy_dest=$(awk '/^copy / { print $NF }' "${tmp}/log")
if [[ "${copy_dest}" != "docker://ghcr.io/awkward8787/fedora-bootc:${pin_tag}" ]]; then
  echo "the copy destination must be the pin tag, not ${copy_dest}" >&2
  exit 1
fi
if [[ "${copy_dest}" == "docker://ghcr.io/awkward8787/fedora-bootc:45" ]]; then
  echo "the copy must not use the shared :45 tag" >&2
  exit 1
fi
if grep -q 'darkjuander' "${tmp}/log"; then
  echo "mirror copy must not name the release image" >&2
  exit 1
fi

# Dropping the tag post-check would accept this.
reset
TAG_AFTER=wrong
rc=0
run_script "${script}" --write >/dev/null 2>&1 || rc=$?
if [[ "${rc}" -eq 0 ]]; then
  echo "a tag that does not match the pin must fail" >&2
  exit 1
fi

# Dropping the digest post-check would accept this.
reset
DIGEST_AFTER=wrong
rc=0
run_script "${script}" --write >/dev/null 2>&1 || rc=$?
if [[ "${rc}" -eq 0 ]]; then
  echo "a digest inspect that does not match the pin must fail" >&2
  exit 1
fi

reset
COPY_MODE=quay_missing
rc=0
err=$(run_script "${script}" --write 2>&1) || rc=$?
if [[ "${rc}" -eq 0 || "${err}" != *"not on Quay"* || "${err}" != *"QUAY-STDERR-MARKER"* ]]; then
  echo "a Quay miss must say not on Quay and show skopeo stderr" >&2
  exit 1
fi
if [[ "${err}" == *"mirror unreachable or tampered"* ]]; then
  echo "a Quay miss must not be reported as a mirror failure" >&2
  exit 1
fi

reset
DIGEST_BEFORE=down
rc=0
err=$(run_script "${script}" --check 2>&1) || rc=$?
if [[ "${rc}" -eq 0 || "${err}" != *"mirror unreachable or tampered"* || "${err}" != *"MIRROR-STDERR-MARKER"* ]]; then
  echo "an unreachable mirror must show skopeo stderr" >&2
  exit 1
fi
if [[ "${err}" == *"not on Quay"* ]]; then
  echo "an unreachable mirror must not be reported as not on Quay" >&2
  exit 1
fi
if [[ -f "${tmp}/log" ]] && grep -q '^copy ' "${tmp}/log"; then
  echo "a failed mirror inspect must not copy" >&2
  exit 1
fi

reset
rc=0
run_script "${script}" --check >/dev/null 2>&1 || rc=$?
if [[ "${rc}" -eq 0 ]]; then
  echo "a check of a missing digest must fail closed" >&2
  exit 1
fi
if [[ -f "${tmp}/log" ]] && grep -q '^copy ' "${tmp}/log"; then
  echo "check mode must not copy" >&2
  exit 1
fi

reset
rc=0
err=$(FEDORA_BOOTC_MIRROR=localhost:5000/org/fedora-bootc run_script "${script}" --write 2>&1) || rc=$?
if [[ "${rc}" -ne 0 ]]; then
  echo "a registry with a port should copy: ${err}" >&2
  exit 1
fi
if ! grep -q "docker://localhost:5000/org/fedora-bootc:${pin_tag}" "${tmp}/log"; then
  echo "a port in the registry host was stripped" >&2
  exit 1
fi
if grep -q 'docker://localhost/org/' "${tmp}/log"; then
  echo "the port was dropped from the registry host" >&2
  exit 1
fi

reset
rc=0
FEDORA_BOOTC_MIRROR=ghcr.io/awkward8787/darkjuander \
  run_script "${script}" --write >/dev/null 2>&1 || rc=$?
if [[ "${rc}" -eq 0 ]]; then
  echo "the release image name must fail" >&2
  exit 1
fi
if [[ -f "${tmp}/log" ]] && grep -q . "${tmp}/log"; then
  echo "the release image guard must run before skopeo" >&2
  exit 1
fi

if grep -Eq 'skopeo delete|untag' "${script}"; then
  echo "the mirror script must not untag an existing pin" >&2
  exit 1
fi

# Dropping the check-mode exit 0 falls through and copies a missing pin tag.
reset
DIGEST_BEFORE=ok
TAG_BEFORE=missing
rc=0
run_script "${script}" >/dev/null 2>&1 || rc=$?
if [[ "${rc}" -ne 0 ]]; then
  echo "a check of a digest that is already mirrored must succeed" >&2
  exit 1
fi
if [[ -f "${tmp}/log" ]] && grep -q '^copy ' "${tmp}/log"; then
  echo "check mode must not copy when the digest is already present" >&2
  exit 1
fi

# No flag is check mode. Making --write the default would copy here.
reset
rc=0
run_script "${script}" >/dev/null 2>&1 || rc=$?
if [[ "${rc}" -eq 0 ]]; then
  echo "the default must stay a read-only check" >&2
  exit 1
fi
if [[ -f "${tmp}/log" ]] && grep -q '^copy ' "${tmp}/log"; then
  echo "the default must not copy" >&2
  exit 1
fi

# The pin tag already points at something else. Do not move it.
reset
DIGEST_BEFORE=ok
TAG_BEFORE=wrong
rc=0
err=$(run_script "${script}" --write 2>&1) || rc=$?
if [[ "${rc}" -eq 0 || "${err}" != *"not moved"* ]]; then
  echo "a pin tag that points elsewhere must be refused and not moved" >&2
  exit 1
fi
if [[ -f "${tmp}/log" ]] && grep -q '^copy ' "${tmp}/log"; then
  echo "refusing to move a pin tag must not copy" >&2
  exit 1
fi

alt=$(printf 'sha256:%s' "$(printf 'c%.0s' {1..64})")
alt_short=${alt#sha256:}
alt_short=${alt_short:0:12}
alt_tag="45-${alt_short}"
reset
rc=0
err=$(FEDORA_BOOTC_DIGEST="${alt}" \
  DIGEST_BEFORE=missing DIGEST_AFTER=ok TAG_BEFORE=missing TAG_AFTER=ok COPY_MODE=ok \
  SKOPEO_LOG="${tmp}/log" SKOPEO_COPIED="${tmp}/copied" \
  EXPECT_DIGEST="${alt}" WRONG_DIGEST="${wrong}" \
  PATH="${tmp}:${PATH}" \
  "${script}" --write 2>&1) || rc=$?
if [[ "${rc}" -ne 0 ]]; then
  echo "a validated digest override should copy: ${err}" >&2
  exit 1
fi
if ! awk '/^copy / { print $NF }' "${tmp}/log" | grep -qx "docker://ghcr.io/awkward8787/fedora-bootc:${alt_tag}"; then
  echo "the override digest must choose its own pin tag" >&2
  exit 1
fi

reset
rc=0
FEDORA_BOOTC_DIGEST=not-a-digest \
  run_script "${script}" --write >/dev/null 2>&1 || rc=$?
if [[ "${rc}" -eq 0 ]]; then
  echo "a digest override that is not sha256 must fail" >&2
  exit 1
fi
if [[ -f "${tmp}/log" ]] && grep -q . "${tmp}/log"; then
  echo "a bad digest override must fail before skopeo" >&2
  exit 1
fi

# Loosening sha256:[0-9a-f]{64} would let these reach skopeo.
for bad in "sha256:$(printf 'a%.0s' {1..63})" "sha256:$(printf 'A%.0s' {1..64})" "sha256:$(printf 'g%.0s' {1..64})"; do
  reset
  rc=0
  FEDORA_BOOTC_DIGEST="${bad}" run_script "${script}" --write >/dev/null 2>&1 || rc=$?
  if [[ "${rc}" -eq 0 ]]; then
    echo "digest ${bad} must fail" >&2
    exit 1
  fi
  if [[ -f "${tmp}/log" ]] && grep -q . "${tmp}/log"; then
    echo "digest ${bad} must fail before skopeo" >&2
    exit 1
  fi
done

reject_index() {
  local mode=$1 label=$2
  reset
  rc=0
  RAW_MODE="${mode}" LABEL_VERSION="${label}" FEDORA_BOOTC_DIGEST="${alt}" \
    DIGEST_BEFORE=missing DIGEST_AFTER=ok TAG_BEFORE=missing TAG_AFTER=ok COPY_MODE=ok \
    SKOPEO_LOG="${tmp}/log" SKOPEO_COPIED="${tmp}/copied" \
    EXPECT_DIGEST="${alt}" WRONG_DIGEST="${wrong}" \
    PATH="${tmp}:${PATH}" \
    "${script}" --write >/dev/null 2>&1 || rc=$?
  if [[ "${rc}" -eq 0 ]]; then
    echo "dispatch index ${mode}/${label} must fail" >&2
    exit 1
  fi
  if [[ -f "${tmp}/log" ]] && grep -q '^copy ' "${tmp}/log"; then
    echo "dispatch index ${mode}/${label} must not be copied" >&2
    exit 1
  fi
}
reject_index single 45.1
reject_index manifest 45.1
reject_index ok 44

reset
rc=0
err=$(RAW_MODE=docker LABEL_VERSION=45 FEDORA_BOOTC_DIGEST="${alt}" \
  DIGEST_BEFORE=missing DIGEST_AFTER=ok TAG_BEFORE=missing TAG_AFTER=ok COPY_MODE=ok \
  SKOPEO_LOG="${tmp}/log" SKOPEO_COPIED="${tmp}/copied" \
  EXPECT_DIGEST="${alt}" WRONG_DIGEST="${wrong}" \
  PATH="${tmp}:${PATH}" \
  "${script}" --write 2>&1) || rc=$?
if [[ "${rc}" -ne 0 ]]; then
  echo "a docker manifest list for 45 should copy: ${err}" >&2
  exit 1
fi

echo "mirror-fedora-bootc: ok"
