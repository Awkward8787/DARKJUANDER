#!/usr/bin/env bash
# The umbrad marker must not pass just because the TTY echoed the probe.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
python3 - "$root" << 'PY'
import sys
sys.path.insert(0, sys.argv[1] + "/scripts")
from smoke_marker import LoginDriver, probe_command, socket_ok

cmd = probe_command("darkjuander")
echoed = cmd + "[dj@fedora ~]$ "
if socket_ok(echoed):
    raise SystemExit("echoed probe must not count as a live socket")
if "UMBRAD_SOCKET_OK" in cmd:
    raise SystemExit("probe command must not contain the marker")
if not socket_ok(echoed + "\nUMBRAD_SOCKET_OK\n"):
    raise SystemExit("a real marker line must pass")

# Missing socket: command echo, failed test, prompt, no marker line.
missing = echoed + "\n[sudo] password for dj: \n[dj@fedora ~]$ "
if socket_ok(missing):
    raise SystemExit("missing socket transcript must fail")

drv = LoginDriver("darkjuander", "Dj-Console-9k")
assert drv.on_text("fedora login: ") == b"dj\n"
assert drv.on_text("Password: ") == b"darkjuander\n"
assert drv.on_text("Current password: ") == b"darkjuander\n"
assert drv.on_text("New password: ") == b"Dj-Console-9k\n"
assert drv.on_text("Retype new password: ") == b"Dj-Console-9k\n"
typed = drv.on_text("[dj@fedora ~]$ ")
assert typed is not None and b"UMBRAD_SOCKET_OK" not in typed
assert b"UMBRAD_SOC" in typed and socket_ok(typed.decode()) is False
print("smoke-marker: ok")
PY