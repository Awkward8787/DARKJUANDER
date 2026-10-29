"""Serial-console checks for the QEMU smoke test.

The guest TTY echoes typed input. A success marker that appears inside the
typed command is therefore not evidence that umbrad is running.
"""

from __future__ import annotations

MARKER = "UMBRAD_SOCKET_OK"
MULTI_MARKER = "MULTI_USER_OK"


def probe_command(sudo_password: str) -> str:
    """Ask the booted guest. quiet/rhgb hides the multi-user banner.

    Each marker is assembled by the guest shell, so the echoed command
    line is not itself a success line.
    """
    return (
        "systemctl is-active multi-user.target | grep -qx active"
        " && printf '%s%s\\n' 'MULTI_' 'USER_OK'; "
        "echo "
        + sudo_password
        + " | sudo -S -p '' test -S /run/darkjuander/umbrad.sock"
        + " && printf '%s%s\\n' 'UMBRAD_SOC' 'KET_OK'\n"
    )


def socket_ok(text: str) -> bool:
    """True only when a transcript line is exactly the socket marker."""
    return any(line.strip() == MARKER for line in text.splitlines())


def password_expired(text: str) -> bool:
    """True when the first login was forced to change dj's password."""
    return (
        "You are required to change your password immediately" in text
        or "password must be changed" in text
    )


def multi_user_ok(text: str) -> bool:
    """True for the systemd banner or a guest-built marker line."""
    if "Reached target multi-user.target" in text or "Multi-User System" in text:
        return True
    return any(line.strip() == MULTI_MARKER for line in text.splitlines())


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
