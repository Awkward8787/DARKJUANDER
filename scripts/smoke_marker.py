"""Serial-console checks for the QEMU smoke test.

The guest TTY echoes typed input. A success marker that appears inside the
typed command is therefore not evidence that umbrad is running.
"""

from __future__ import annotations

MARKER = "UMBRAD_SOCKET_OK"


def probe_command(sudo_password: str) -> str:
    """Build the socket probe. The marker is assembled by the guest shell."""
    return (
        "echo "
        + sudo_password
        + " | sudo -S -p '' test -S /run/darkjuander/umbrad.sock"
        + " && printf '%s%s\\n' 'UMBRAD_SOC' 'KET_OK'\n"
    )


def socket_ok(text: str) -> bool:
    """True only when a transcript line is exactly the marker.

    The echoed probe contains the two printf pieces, not this line.
    """
    return any(line.strip() == MARKER for line in text.splitlines())


class LoginDriver:
    """What to type next, including an expired-password change."""

    def __init__(self, old_password: str, new_password: str) -> None:
        self.old = old_password
        self.new = new_password
        self.sent_user = False
        self.sent_login_pw = False
        self.sent_current = False
        self.sent_new = False
        self.sent_retype = False
        self.sent_cmd = False
        self.second_login = False

    def sudo_password(self) -> str:
        if self.sent_retype:
            return self.new
        return self.old

    def on_text(self, text: str) -> bytes | None:
        # Only the latest prompt. The transcript keeps earlier "login:" lines.
        tail = text[-80:]
        if "login:" in tail and (
            not self.sent_user or (self.sent_retype and not self.second_login)
        ):
            if self.sent_retype:
                self.second_login = True
                self.sent_login_pw = False
            self.sent_user = True
            return b"dj\n"
        if "New password" in tail and not self.sent_new:
            self.sent_new = True
            return (self.new + "\n").encode()
        if "Retype" in tail and not self.sent_retype:
            self.sent_retype = True
            return (self.new + "\n").encode()
        if "Current password" in tail and not self.sent_current:
            self.sent_current = True
            return (self.old + "\n").encode()
        if (
            "assword:" in tail
            and "New password" not in tail
            and "Current password" not in tail
            and "Retype" not in tail
            and not self.sent_login_pw
        ):
            self.sent_login_pw = True
            pw = self.new if self.second_login else self.old
            return (pw + "\n").encode()
        if (not self.sent_cmd) and ("~]$" in tail or "~]#" in tail):
            self.sent_cmd = True
            return probe_command(self.sudo_password()).encode()
        return None
