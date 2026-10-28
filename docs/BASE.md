# dj-base

`image/Containerfile` starts from `quay.io/fedora/fedora-bootc:45` pinned by the index digest resolved on 2026-10-28 (`sha256:ed6ebf922c4c63704b1ff7befff74c8d06a5fb74a37b817df3d2cd093568bbc5`). That tag is a manifest list. On that date Quay also had `45-aarch64` and `45-x86_64`. Build on the target architecture. The Containerfile copies the `umbrad` and `dj` binaries you staged, so a macOS `cargo build` must not be copied into the image. The image is not signed.

What the image definition configures:

- `dj-auditd.service` as user `dj-audit`. The signing seed and the signer's copy of the chain stay in `/var/lib/dj-audit` (mode `0700`, seed mode `0600`). The verifying key is published at `/run/dj-audit/audit.ed25519.pub` so `umbrad` can read it without listing the seed directory. `umbrad` can connect to `/run/dj-audit/signer.sock`. The signer accepts only uid `dj-umbra` and only a record that extends its last signed head. Offline verify still accepts an older log when that log is restored together with the checkpoint signed for it.
- `umbrad.service` in `umbra-weights.slice`, with systemd sandboxing and localhost-only IP filtering. It starts after `dj-auditd`.
- nftables default-deny inbound, loopback and ICMP accepted, forwarding dropped.
- sysctl: hide kernel pointers, restrict dmesg, disable unprivileged BPF, restrict ptrace, disable kexec.
- sshd disabled when the unit exists. The rpm-ostree count-me timer is masked when it exists.
- `dj-llama.service` installed and not enabled.
- Umbra Eclipse branding, via `image/apply-branding.sh`. This has not been booted. The build fails if a listed asset is missing or empty.
  - Plymouth theme `darkjuander` at `/usr/share/plymouth/themes/darkjuander`, set as the default with `plymouth-set-default-theme`. `image/dracut/darkjuander-plymouth.conf` adds the plymouth module for a later initramfs build. The container build does not run dracut. The logo and wordmark are PNGs. The passphrase and status lines in the script ask for the Inter font, and whether the initramfs can resolve that font has not been tested.
  - GRUB theme at `/usr/share/grub/themes/darkjuander`. `grub2-mkfont` writes `inter_regular_18.pf2` and `jetbrains_mono_16.pf2` during the image build, from the OFL fonts instantiated at the weights in `branding/scripts/build.py`. `GRUB_THEME` in `/etc/default/grub` points at that `theme.txt`. bootupd owns the bootloader on bootc, and GRUB may only read a theme from the boot partition. That has not been tested.
  - Wallpapers at `/usr/share/backgrounds/darkjuander/`. `default.png` and `/usr/share/backgrounds/default.png` point at `darkjuander-3840x2160.png`. `/usr/share/gnome-background-properties/darkjuander.xml` names the same file. No desktop or greeter is installed, so nothing displays it yet.
  - hicolor icon `darkjuander` under `/usr/share/icons/hicolor/`. `LOGO=darkjuander` uses that name.
  - Terminal color schemes under `/usr/share/darkjuander/terminal/`.
  - OFL fonts and their license files under `/usr/share/fonts/darkjuander/`.
  - `/usr/lib/os-release`: `NAME="DARKJUANDER"`, `PRETTY_NAME="DARKJUANDER"`, `LOGO=darkjuander`, `HOME_URL="https://github.com/Awkward8787/DARKJUANDER"`, `ID_LIKE=fedora`. Other fields, including `ID`, stay as the Fedora base image set them.
  - `/etc/issue` is the login greeting from the brand guide: "Welcome back. Your agents are where you left them." The status motd in that guide is marked planned, and this image does not install it.

What it does not do:

- It does not sign the image.
- It does not enroll Secure Boot or seal a LUKS key to a TPM.
- It does not load `image/selinux/darkjuander.te`.
- CI has not run `podman build` or `bootc-image-builder`.
- The Plymouth theme, GRUB theme, wallpaper, and `/etc/issue` greeting have not been seen on a boot.

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
