#!/usr/bin/env bash
# Fill docs/RELEASE_NOTES_TEMPLATE.md for one version.
# stdout is the release body. ROADMAP.md is included unchanged.
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "usage: render-release-notes.sh VERSION [smoke-result-file]" >&2
  exit 1
fi

version=$1
smoke=${2:-}
root=$(cd "$(dirname "$0")/.." && pwd)
template=${root}/docs/RELEASE_NOTES_TEMPLATE.md
roadmap=${root}/docs/ROADMAP.md

if [[ ! -f "${template}" || ! -f "${roadmap}" ]]; then
  echo "missing template or roadmap" >&2
  exit 1
fi

smoke_text="No smoke result was passed to this render."
if [[ -n "${smoke}" && -f "${smoke}" ]]; then
  smoke_text=$(cat "${smoke}")
fi

roadmap_text=$(cat "${roadmap}")
body=$(cat "${template}")
body=${body//__VERSION__/${version}}
body=${body//__SMOKE__/${smoke_text}}
body=${body//__ROADMAP__/${roadmap_text}}
printf '%s\n' "${body}"
