#!/usr/bin/env bash
# Fail unless the docker config cosign reads has a ghcr.io user:token.
# cosign login writes this file for the runner user. Root's podman auth
# file is a different path and does not satisfy this check. A credsStore
# or a ghcr.io credHelper makes clients ignore the inline auth entry.
set -euo pipefail

dir=${DOCKER_CONFIG:-${HOME}/.docker}
config=${dir}/config.json
if [[ ! -f "${config}" ]]; then
  echo "cosign docker config is missing at ${config}" >&2
  exit 1
fi
python3 - "${config}" << 'PY'
import base64
import binascii
import json
import sys

path = sys.argv[1]
with open(path, encoding="utf-8") as fh:
    data = json.load(fh)
if data.get("credsStore"):
    sys.stderr.write("docker config sets credsStore; inline ghcr.io auth would be ignored\n")
    raise SystemExit(1)
helpers = data.get("credHelpers") or {}
if isinstance(helpers, dict) and "ghcr.io" in helpers:
    sys.stderr.write("docker config sets credHelpers for ghcr.io\n")
    raise SystemExit(1)
auths = data.get("auths", {})
if not isinstance(auths, dict):
    sys.stderr.write("docker config auths is not an object\n")
    raise SystemExit(1)
entry = auths.get("ghcr.io")
if not isinstance(entry, dict):
    sys.stderr.write("docker config has no ghcr.io auth entry\n")
    raise SystemExit(1)
auth = entry.get("auth", "")
if not isinstance(auth, str) or auth.strip() == "":
    sys.stderr.write("docker config has no ghcr.io auth entry\n")
    raise SystemExit(1)
try:
    raw = base64.b64decode(auth, validate=True)
except (binascii.Error, ValueError):
    sys.stderr.write("ghcr.io auth entry is not valid base64\n")
    raise SystemExit(1)
try:
    text = raw.decode("utf-8")
except UnicodeDecodeError:
    sys.stderr.write("ghcr.io auth entry is not a user:token string\n")
    raise SystemExit(1)
user, sep, secret = text.partition(":")
if sep != ":" or user.strip() == "" or secret.strip() == "":
    sys.stderr.write("ghcr.io auth entry must be base64 of user:token\n")
    raise SystemExit(1)
PY
echo "docker config at ${config} has a ghcr.io user:token"
