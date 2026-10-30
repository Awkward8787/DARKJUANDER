#!/usr/bin/env bash
# Decide the release version from the git ref. Tag names are validated
# before anything interpolates them into a later shell step.
set -euo pipefail

ref=${GITHUB_REF:?}
sha=${GITHUB_SHA:?}
event=${GITHUB_EVENT_NAME:?}
dry=${DRY_RUN:-}
out=${GITHUB_OUTPUT:?}

# No leading zeros, so v1.02.0 is not a version and cannot tie with v1.2.0.
tag_re='^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z.]+)?$'
dry_re='^0\.0\.0-dry\.[0-9a-f]{12}$'

publish=false
if [[ "${ref}" == refs/tags/* ]]; then
  ver=${ref#refs/tags/}
  if [[ ! "${ver}" =~ ${tag_re} ]]; then
    echo "refusing tag name that is not a dotted version" >&2
    exit 1
  fi
  # workflow_dispatch on a tag publishes only when dry_run is explicitly false.
  # A tag push has no dry_run input and does publish.
  if [[ "${event}" != "workflow_dispatch" || "${dry}" == "false" ]]; then
    publish=true
  fi
else
  ver="0.0.0-dry.${sha:0:12}"
  if [[ ! "${ver}" =~ ${dry_re} ]]; then
    echo "refusing dry-run version" >&2
    exit 1
  fi
fi

printf 'version=%s\n' "${ver}" >> "${out}"
printf 'publish=%s\n' "${publish}" >> "${out}"
echo "version=${ver} publish=${publish}"
