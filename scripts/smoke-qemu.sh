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
    machine=( -machine virt -cpu cortex-a57 )
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
if [[ -r /dev/kvm ]]; then
  accel=kvm
  if [[ "${arch}" == "x86_64" ]]; then
    machine=( -machine "q35,accel=kvm" -cpu host )
  else
    machine=( -machine "virt,accel=kvm" -cpu host )
  fi
fi

timeout_secs=${SMOKE_TIMEOUT_SECS:-1200}
echo "smoke: ${qemu} accel=${accel} timeout=${timeout_secs}s image=${image}"

set +e
python3 - "${qemu}" "${firmware}" "${image}" "${log}" "${result}" "${password}" "${timeout_secs}" \
  "${machine[@]}" -- "${netdev[@]}" << 'PY'
import os
import selectors
import subprocess
import sys
import time

qemu = sys.argv[1]
firmware = sys.argv[2]
image = sys.argv[3]
log_path = sys.argv[4]
result_path = sys.argv[5]
password = sys.argv[6]
timeout = int(sys.argv[7])
# argv after the fixed fields: machine... -- netdev...
sep = sys.argv.index("--", 8)
machine = sys.argv[8:sep]
netdev = sys.argv[sep + 1 :]

cmd = [
    qemu,
    *machine,
    "-m",
    "2048",
    "-smp",
    "2",
    "-drive",
    f"if=pflash,format=raw,readonly=on,file={firmware}",
    "-drive",
    f"file={image},format=qcow2,if=virtio",
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
sent_user = False
sent_pass = False
sent_cmd = False
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
            text = buf.decode("utf-8", "replace")
            tail = text[-400:]
            if "Reached target Multi-User System" in text or "Reached target multi-user" in text:
                multi = True
            if (not sent_user) and "login:" in tail:
                proc.stdin.write(b"dj\n")
                proc.stdin.flush()
                sent_user = True
            if sent_user and (not sent_pass) and "assword:" in tail:
                proc.stdin.write(password.encode() + b"\n")
                proc.stdin.flush()
                sent_pass = True
            if sent_pass and (not sent_cmd) and ("~]$" in tail or "~]#" in tail):
                cmd_line = (
                    "echo "
                    + password
                    + " | sudo -S -p '' test -S /run/darkjuander/umbrad.sock"
                    + " && echo UMBRAD_SOCKET_OK\n"
                )
                proc.stdin.write(cmd_line.encode())
                proc.stdin.flush()
                sent_cmd = True
            if "UMBRAD_SOCKET_OK" in text:
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
    f"login-reached: {'yes' if sent_user else 'no'}",
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
