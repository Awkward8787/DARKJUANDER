#!/usr/bin/env bash
# Fail unless the docker config cosign reads has a non-empty ghcr.io credential.
# cosign login writes this file for the runner user. Root's podman auth file
# is a different path and does not satisfy this check.
set -euo pipefail

dir=${DOCKER_CONFIG:-${HOME}/.docker}
config=${dir}/config.json
if [[ ! -f "${config}" ]]; then
  echo "cosign docker config is missing at ${config}" >&2
  exit 1
fi
python3 - "${config}" << 'PY'
import json
import sys

path = sys.argv[1]
with open(path, encoding="utf-8") as fh:
    data = json.load(fh)
auth = data.get("auths", {}).get("ghcr.io", {}).get("auth", "")
if not isinstance(auth, str) or auth == "":
    sys.stderr.write("docker config has no ghcr.io auth entry\n")
    raise SystemExit(1)
PY
echo "docker config at ${config} has a ghcr.io auth entry"
