#!/usr/bin/env bash
# Report whether a kernel config has the options UMBRA's Phase 0 notes care about.
# PSI, cgroup v2, and BPF are required. sched_ext and Landlock are reported and do not fail the script.
# Usage: check-kernel-config.sh [config-file]
# With no file, reads /proc/config.gz or /boot/config-$(uname -r).
set -euo pipefail

required=(CONFIG_PSI CONFIG_CGROUPS CONFIG_MEMCG)
bpf_any=(CONFIG_BPF_SYSCALL CONFIG_BPF)
warn=(CONFIG_DEBUG_INFO_BTF CONFIG_SCHED_CLASS_EXT CONFIG_SECURITY_LANDLOCK CONFIG_SECURITY_SELINUX CONFIG_BPF_UNPRIV_DEFAULT_OFF)

tmp=""
cleanup() {
  if [[ -n "${tmp}" ]]; then
    rm -f "${tmp}"
  fi
}
trap cleanup EXIT

if [[ $# -ge 1 ]]; then
  cfg=$1
else
  if [[ -r /proc/config.gz ]]; then
    tmp=$(mktemp)
    gzip -dc /proc/config.gz >"${tmp}"
    cfg=${tmp}
  elif [[ -r "/boot/config-$(uname -r)" ]]; then
    cfg="/boot/config-$(uname -r)"
  else
    echo "no kernel config found. Pass a file, or boot with /proc/config.gz." >&2
    exit 2
  fi
fi

if [[ ! -r "${cfg}" ]]; then
  echo "cannot read ${cfg}" >&2
  exit 2
fi

value_of() {
  local key=$1
  local line
  line=$(grep -E "^${key}=" "${cfg}" | tail -n 1 || true)
  if [[ -z "${line}" ]]; then
    if grep -q "^# ${key} is not set" "${cfg}"; then
      echo "not-set"
    else
      echo "absent"
    fi
  else
    echo "${line#*=}"
  fi
}

fail=0
echo "kernel config: ${cfg}"
for key in "${required[@]}"; do
  val=$(value_of "${key}")
  printf '  %-32s %s\n' "${key}" "${val}"
  if [[ "${val}" != "y" ]]; then
    fail=1
  fi
done

bpf_ok=0
for key in "${bpf_any[@]}"; do
  val=$(value_of "${key}")
  printf '  %-32s %s\n' "${key}" "${val}"
  if [[ "${val}" == "y" ]]; then
    bpf_ok=1
  fi
done
if [[ "${bpf_ok}" -ne 1 ]]; then
  fail=1
fi

for key in "${warn[@]}"; do
  val=$(value_of "${key}")
  printf '  %-32s %s\n' "${key}" "${val}"
done

if [[ "${fail}" -ne 0 ]]; then
  echo "missing a Phase 0 kernel option (PSI, cgroups, memcg, or BPF)." >&2
  exit 1
fi
echo "Phase 0 kernel options are present. sched_ext and Landlock lines above are informational."
