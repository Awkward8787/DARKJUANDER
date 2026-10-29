#!/usr/bin/env bash
# Boot a qcow2 and look for multi-user plus the umbrad socket.
# Uses KVM when /dev/kvm is usable, otherwise TCG. Exits 0 only when both
# checks passed. Always writes the serial log next to the image as
# <image>.serial.log and a one-line result file <image>.smoke.txt.
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "usage: smoke-qemu.sh IMAGE.qcow2" >&2
  exit 1
fi

image=$1
if [[ ! -f "${image}" ]]; then
  echo "missing ${image}" >&2
  exit 1
fi

log=${image}.serial.log
result=${image}.smoke.txt
password_file=${DJ_CONSOLE_PASSWORD_FILE:-}
root=$(cd "$(dirname "$0")/.." && pwd)
password_file=${password_file:-${root}/image/console-password.txt}
password=$(grep -v '^[[:space:]]*#' "${password_file}" | head -n 1 | tr -d '\r\n')
if [[ -z "${password}" ]]; then
  echo "console password file is empty" >&2
  exit 1
fi

arch=$(uname -m)
accel=tcg
case "${arch}" in
  aarch64|arm64)
    qemu="qemu-system-aarch64"
    # GitHub-hosted ARM runners do not expose /dev/kvm (nested virt is not
    # available on the Azure ARM fleet). cortex-a57 is the small TCG model;
    # -cpu max emulates SVE and pointer auth and is slower. KVM, when the
    # device is actually readable, replaces this below.
    machine=( -machine virt -cpu cortex-a57 -accel "tcg,thread=multi,tb-size=256" )
    firmware=/usr/share/AAVMF/AAVMF_CODE.fd
    if [[ ! -f "${firmware}" ]]; then
      firmware=/usr/share/qemu-efi-aarch64/QEMU_EFI.fd
    fi
    netdev=( -netdev "user,id=n0" -device "virtio-net-device,netdev=n0" )
    ;;
  x86_64)
    qemu="qemu-system-x86_64"
    firmware=/usr/share/OVMF/OVMF_CODE_4M.fd
    if [[ ! -f "${firmware}" ]]; then
      firmware=/usr/share/OVMF/OVMF_CODE.fd
    fi
    machine=( -machine q35 -cpu qemu64 )
    netdev=( -netdev "user,id=n0" -device "virtio-net-pci,netdev=n0" )
    ;;
  *)
    echo "unsupported smoke arch ${arch}" | tee "${result}"
    exit 1
    ;;
esac

if [[ ! -x "$(command -v "${qemu}")" ]]; then
  echo "smoke: ${qemu} is not installed" | tee "${result}"
  exit 1
fi
if [[ ! -f "${firmware}" ]]; then
  echo "smoke: firmware not found at ${firmware}" | tee "${result}"
  exit 1
fi
if [[ -e /dev/kvm ]]; then
  echo "smoke: /dev/kvm $(stat -c '%A %U:%G' /dev/kvm 2>/dev/null || echo present)"
else
  echo "smoke: /dev/kvm absent"
fi
if [[ -r /dev/kvm ]]; then
  accel=kvm
  if [[ "${arch}" == "x86_64" ]]; then
    machine=( -machine "q35,accel=kvm" -cpu host )
  else
    machine=( -machine "virt,accel=kvm" -cpu host )
  fi
fi

# Hard cap. The ARM runner has no KVM, so this used to sit in TCG until a
# two-hour job timeout. The default is short on purpose: a hung guest fails
# the step instead of occupying the runner.
timeout_secs=${SMOKE_TIMEOUT_SECS:-600}
echo "smoke: ${qemu} accel=${accel} timeout=${timeout_secs}s firmware=${firmware} image=${image}"

set +e
# python enforces the deadline and writes the result file. timeout(1) is the
# backstop if that process stops checking the clock. It is longer than the
# deadline so a normal timeout still records multi-user/socket lines.
export SMOKE_LIB="${root}/scripts"
# First-login password chosen only inside this VM. The shipped default is
# expired by dj-expire-password.service, so the guest asks for a new one.
smoke_new_password=${SMOKE_NEW_PASSWORD:-Dj-Console-9k}
timeout --foreground --kill-after=20 "$((timeout_secs + 45))" \
  python3 - "${qemu}" "${firmware}" "${image}" "${log}" "${result}" "${password}" "${timeout_secs}" "${smoke_new_password}" \
  "${machine[@]}" -- "${netdev[@]}" << 'PY'
import os
import re
import selectors
import subprocess
import sys
import time

sys.path.insert(0, os.environ["SMOKE_LIB"])
from smoke_marker import LoginDriver, socket_ok as marker_ok

# systemd colors the console, so "Reached target multi-user.target" is not
# contiguous bytes. Match the phrase after stripping those sequences.
ANSI = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)")

qemu = sys.argv[1]
firmware = sys.argv[2]
image = sys.argv[3]
log_path = sys.argv[4]
result_path = sys.argv[5]
password = sys.argv[6]
timeout = int(sys.argv[7])
new_password = sys.argv[8]
# argv after the fixed fields: machine... -- netdev...
sep = sys.argv.index("--", 9)
machine = sys.argv[9:sep]
netdev = sys.argv[sep + 1 :]
login = LoginDriver(password, new_password)

# 2G/2cpu is enough for multi-user and cheaper to emulate than a larger VM.
# cache=unsafe is smoke-only: the qcow2 is discarded, and the socket check
# is on a tmpfs. virtio-rng keeps early boot from blocking on the entropy pool.
mem = "2048"
smp = "2"
cmd = [
    qemu,
    *machine,
    "-m",
    mem,
    "-smp",
    smp,
    "-drive",
    f"if=pflash,format=raw,readonly=on,file={firmware}",
    "-drive",
    f"file={image},format=qcow2,if=virtio,cache=unsafe",
    "-device",
    "virtio-rng-pci",
    "-nographic",
    "-serial",
    "mon:stdio",
    *netdev,
]

proc = subprocess.Popen(
    cmd,
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.STDOUT,
    bufsize=0,
)
sel = selectors.DefaultSelector()
sel.register(proc.stdout, selectors.EVENT_READ)
log = open(log_path, "wb")
buf = b""
multi = False
socket_ok = False
deadline = time.time() + timeout
try:
    while time.time() < deadline:
        if proc.poll() is not None and not sel.get_map():
            break
        events = sel.select(timeout=1)
        if not events and proc.poll() is not None:
            break
        for key, _ in events:
            chunk = os.read(key.fileobj.fileno(), 4096)
            if not chunk:
                sel.unregister(key.fileobj)
                continue
            log.write(chunk)
            log.flush()
            buf += chunk
            text = ANSI.sub("", buf.decode("utf-8", "replace"))
            if "Reached target multi-user.target" in text or "Multi-User System" in text:
                multi = True
            typed = login.on_text(text)
            if typed:
                proc.stdin.write(typed)
                proc.stdin.flush()
            if marker_ok(text):
                socket_ok = True
        if socket_ok:
            break
finally:
    proc.terminate()
    try:
        proc.wait(timeout=15)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait(timeout=15)
    log.close()

lines = [
    f"multi-user: {'yes' if multi else 'no'}",
    f"umbrad-socket: {'yes' if socket_ok else 'no'}",
    f"login-reached: {'yes' if login.sent_user else 'no'}",
]
text = "\n".join(lines) + "\n"
sys.stderr.write(text)
open(result_path, "w", encoding="utf-8").write(text)
sys.exit(0 if multi and socket_ok else 1)
PY
status=$?
echo "smoke exit ${status}"
cat "${result}" || true
exit "${status}"
