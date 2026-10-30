#!/usr/bin/env bash
# The umbrad marker must not pass just because the TTY echoed the probe.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
python3 - "$root" << 'PY'
import sys
sys.path.insert(0, sys.argv[1] + "/scripts")
from smoke_marker import (
    LoginDriver,
    expiry_proven,
    multi_user_ok,
    password_expired,
    probe_command,
    socket_ok,
)

cmd = probe_command("darkjuander")
echoed = cmd + "[dj@fedora ~]$ "
if socket_ok(echoed) or multi_user_ok(echoed):
    raise SystemExit("echoed probe must not count as a live boot")
if "UMBRAD_SOCKET_OK" in cmd or "MULTI_USER_OK" in cmd:
    raise SystemExit("probe command must not contain a marker")
if "is-system-running --wait" not in cmd:
    raise SystemExit("probe must wait until the system is running")
if not socket_ok(echoed + "\nUMBRAD_SOCKET_OK\n"):
    raise SystemExit("a real socket marker line must pass")
if not multi_user_ok(echoed + "\nMULTI_USER_OK\n"):
    raise SystemExit("a real multi-user marker line must pass")
if not multi_user_ok("Reached target multi-user.target - Multi-User System."):
    raise SystemExit("the console banner must still count")

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
plain = "darkjuander login: dj\nPassword: \n[dj@darkjuander ~]$ "
if password_expired(plain):
    raise SystemExit("a login without an expiry prompt must fail")
forced = plain + "You are required to change your password immediately (administrator enforced).\n"
if not password_expired(forced):
    raise SystemExit("the forced password change must count")


def drive(chunks, old="darkjuander", new="Dj-Console-9k"):
    guest = LoginDriver(old, new)
    buf = ""
    sent_new = False
    for chunk in chunks:
        buf += chunk
        typed = guest.on_text(buf)
        if (
            typed
            and guest.sent_retype
            and guest.sent_cmd
            and new.encode() in typed
        ):
            sent_new = True
    accepted = sent_new and socket_ok(buf)
    return expiry_proven(buf, guest.sent_retype, accepted)


# chage -l prints this on an account that is already expired, and a
# non-expired guest can print the same line. The phrase is not the proof.
chage = "Last password change\t\t\t: password must be changed\n"
hostile = [
    "darkjuander login: ",
    "Password: ",
    chage,
    "[dj@darkjuander ~]$ ",
    "MULTI_USER_OK\nUMBRAD_SOCKET_OK\n",
]
if not password_expired("".join(hostile)):
    raise SystemExit("the chage line is the phrase the old check trusted")
if drive(hostile):
    raise SystemExit("a non-expired guest printing the chage line must fail")

changed = [
    "darkjuander login: ",
    "Password: ",
    "You are required to change your password immediately (administrator enforced).\n",
    "Current password: ",
    "New password: ",
    "Retype new password: ",
    "[dj@darkjuander ~]$ ",
    "MULTI_USER_OK\nUMBRAD_SOCKET_OK\n",
]
if not drive(changed):
    raise SystemExit("a finished change that sudo accepts must pass")
print("smoke-marker: ok")
PY