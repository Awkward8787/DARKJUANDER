# dj-base

`image/Containerfile` starts from `quay.io/fedora/fedora-bootc:45` pinned by the index digest resolved on 2026-10-28 (`sha256:ed6ebf922c4c63704b1ff7befff74c8d06a5fb74a37b817df3d2cd093568bbc5`). That tag is a manifest list. On that date Quay also had `45-aarch64` and `45-x86_64`. Build on the target architecture. The Containerfile copies the `umbrad` and `dj` binaries you staged, so a macOS `cargo build` must not be copied into the image. The image is not signed.

What the image definition configures:

- `dj-auditd.service` as user `dj-audit`. The signing seed stays in `/var/lib/dj-audit` (mode `0751`, seed mode `0600`). `umbrad` can connect to `/run/dj-audit/signer.sock` and can read the published verifying key. It cannot write that directory.
- `umbrad.service` in `umbra-weights.slice`, with systemd sandboxing and localhost-only IP filtering. It starts after `dj-auditd`.
- nftables default-deny inbound, loopback and ICMP accepted, forwarding dropped.
- sysctl: hide kernel pointers, restrict dmesg, disable unprivileged BPF, restrict ptrace, disable kexec.
- sshd disabled when the unit exists. The rpm-ostree count-me timer is masked when it exists.
- `dj-llama.service` installed and not enabled.

What it does not do:

- It does not sign the image.
- It does not enroll Secure Boot or seal a LUKS key to a TPM.
- It does not load `image/selinux/darkjuander.te`.
- CI has not run `podman build` or `bootc-image-builder`.

## Fedora 45 kernel options

Read on 2026-10-28 from the Fedora dist-git `f45` branch, not from a booted guest:

| Option | aarch64 | x86_64 |
|---|---|---|
| `CONFIG_PSI` | y | y |
| `CONFIG_CGROUPS` | y | y |
| `CONFIG_MEMCG` | y | y |
| `CONFIG_BPF_SYSCALL` | y | y |
| `CONFIG_DEBUG_INFO_BTF` | y | y |
| `CONFIG_SCHED_CLASS_EXT` | y | y |
| `CONFIG_SECURITY_LANDLOCK` | y | y |
| `CONFIG_SECURITY_SELINUX` | y | y |
| `CONFIG_BPF_UNPRIV_DEFAULT_OFF` | y | y |

Sources:

- `https://src.fedoraproject.org/rpms/kernel/raw/f45/f/kernel-aarch64-fedora.config`
- `https://src.fedoraproject.org/rpms/kernel/raw/f45/f/kernel-x86_64-fedora.config`

`scripts/check-kernel-config.sh` is what to run on a booted guest. Re-run it there before treating the table above as the guest's config. A bootc image can ship a newer kernel than the dist-git file that was read.

## Disk images

`scripts/build-disk.sh` calls `quay.io/centos-bootc/bootc-image-builder` with `--type raw` and `--type qcow2`. It requires an SSH `authorized_keys` file and does not set a password. The guest firewall drops inbound SSH, so the first login path is the hypervisor console.
