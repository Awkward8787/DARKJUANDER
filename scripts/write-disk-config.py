#!/usr/bin/env python3
"""Write the bootc-image-builder config for one image type.

qcow2 creates user dj in the image customization. anaconda-iso cannot
combine that table with a kickstart, so the user is created in the
kickstart. The kickstart also turns Fedora dnf repos back off after
Anaconda has copied the depsolve layer onto the installed system.
"""

import subprocess
import sys


def main() -> None:
    if len(sys.argv) != 6:
        print(
            "usage: write-disk-config.py MODE PASSWORD CONSOLE BOOT_UNITS OUT",
            file=sys.stderr,
        )
        raise SystemExit(2)
    mode, password, console, boot_units, out = sys.argv[1:]
    kernel = (
        "[customizations.kernel]\n"
        f'append = "{console} {boot_units}"\n'
    )
    if mode == "qcow2":
        body = (
            "[[customizations.user]]\n"
            'name = "dj"\n'
            f'password = "{password}"\n'
            'groups = ["wheel"]\n'
            "\n"
            + kernel
        )
    elif mode == "anaconda-iso":
        hashed = subprocess.check_output(
            ["openssl", "passwd", "-6", password], text=True
        ).strip()
        kickstart = f"""user --name=dj --groups=wheel --iscrypted --password={hashed}
%post --erroronfail
set -eu
for f in /etc/yum.repos.d/*.repo /usr/lib/yum.repos.d/*.repo; do
  [ -e "$f" ] || continue
  sed -i -E 's/^enabled[[:space:]]*=[[:space:]]*1/enabled=0/' "$f"
done
rm -f /etc/yum.repos.d/fedora-darkjuander.repo
%end
"""
        body = (
            kernel
            + "\n[customizations.installer.kickstart]\n"
            + "contents = '''\n"
            + kickstart
            + "'''\n"
        )
    else:
        print(f"unsupported mode {mode}", file=sys.stderr)
        raise SystemExit(2)
    with open(out, "w", encoding="utf-8") as fh:
        fh.write(body)


if __name__ == "__main__":
    main()
