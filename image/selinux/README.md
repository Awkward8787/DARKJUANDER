# SELinux

`darkjuander.te` is a draft. It is not compiled into a policy package, and the
image does not load it. `dj-base` keeps Fedora's enforcing mode and does not
add a DARKJUANDER domain.

`scripts/check-selinux.sh` runs `checkmodule` when `selinux-policy-devel` is
installed. A failure there means the draft still needs work against the Fedora
base policy. It does not mean a domain is active.
