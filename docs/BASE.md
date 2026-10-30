# dj-base

`image/Containerfile` builds from `ghcr.io/awkward8787/fedora-bootc@sha256:479ad7d16893891ea21c774536b11e26cd5b87fa59636a902700cbe438cee634`. That is the Fedora bootc 45 index Quay served for `quay.io/fedora/fedora-bootc:45` on 2026-10-30, and it is the only pin, in `image/fedora-bootc.digest`. A push to `main`, or a workflow dispatch from `main`, may copy that index with `skopeo copy --all --preserve-digests` to the tag `45-` plus the first 12 hex digits of the digest. That tag is not moved onto a different digest, and the script does not delete other tags. Pull requests, other branches, and `v*` tag builds only check that the digest is already in the mirror. The Containerfile uses the digest. This package is not `ghcr.io/awkward8787/darkjuander`. The index includes aarch64 and x86_64. Build on the target architecture. The Containerfile copies the `umbrad` and `dj` binaries you staged, so a macOS `cargo build` must not be copied into the image. The image is not signed.

What the image definition configures:

- `dj-auditd.service` as user `dj-audit`. The signing seed and the last signed head stay in `/var/lib/dj-audit` (mode `0700`, seed mode `0600`). A durable copy of the verifying key is written there too, at `audit.ed25519.pub`, for offline `dj audit verify` of a copied log. The same key is published at `/run/dj-audit/audit.ed25519.pub` so `umbrad` can read it without listing the seed directory. `umbrad` can connect to `/run/dj-audit/signer.sock`. The signer accepts only uid `dj-umbra`, only a few requests at a time, and only a record that extends its last signed head. Offline verify still accepts an older log when that log is restored together with the checkpoint signed for it.
- `umbrad.service` in `umbra-weights.slice`, with systemd sandboxing and localhost-only IP filtering. It starts after `dj-auditd`.
- nftables default-deny inbound, loopback and ICMP accepted, forwarding dropped.
- sysctl: hide kernel pointers, restrict dmesg, disable unprivileged BPF, restrict ptrace, disable kexec.
- sshd disabled when the unit exists. The rpm-ostree count-me timer is masked when it exists.
- `dj-llama.service` installed and not enabled.
- Umbra Eclipse branding, via `image/apply-branding.sh`. The build fails if a listed asset is missing or empty. CI runs `podman build` on this Containerfile with stub `umbrad`, `dj`, and `dj-auditd` binaries, then `image/check-initramfs.sh` requires the theme script path, `Theme=darkjuander` inside the initramfs, and `Plymouth.ttf` pointing at Inter. That job does not boot the image, and the stubs are not the release binaries.
  - Plymouth theme `darkjuander` at `/usr/share/plymouth/themes/darkjuander`, set as the default. The Containerfile runs `dracut` and replaces `/usr/lib/modules/*/initramfs.img`. `image/check-initramfs.sh` requires `usr/share/plymouth/themes/darkjuander/darkjuander.script` in the listing, `Theme=darkjuander` from `lsinitrd -f etc/plymouth/plymouthd.conf`, and `Plymouth.ttf` pointing at Inter. It fails if the initramfs is over 140 MiB. `plymouth-plugin-script` and `plymouth-plugin-label` (`label-freetype.so`) are installed. label-freetype ignores the family in `Image.Text` and draws with `Plymouth.ttf`. `50-darkjuander-inter.conf` is installed ahead of Noto so that file is Inter, and the build checks `fc-match` before dracut. `UseSimpledrm=1` is appended to `/usr/share/plymouth/plymouthd.defaults` because the plymouth dracut module reads that file when hostonly is off. That keeps the initramfs near the base size instead of pulling in nvidia firmware. simpledrm is in the Fedora kernel. A UEFI framebuffer (UTM) is the expected splash path. This has not been booted. `/usr/lib/bootc/kargs.d/10-darkjuander.toml` sets `rhgb` and `quiet`.
  - GRUB artwork in `branding/grub/` is not installed. bootupd's static config does not read a theme, `/etc/default/grub`, or `/usr/share/grub/themes`.
  - Wallpapers at `/usr/share/backgrounds/darkjuander/`, generated during the image build. `default.png` points at the 3840x2160 file. `/usr/share/gnome-background-properties/darkjuander.xml` names the same file. No desktop or greeter is installed, so nothing displays the wallpaper or the terminal color schemes yet. Those files are still installed.
  - hicolor icon `darkjuander` under `/usr/share/icons/hicolor/`, plus `/usr/share/pixmaps/darkjuander.png`. `LOGO=darkjuander` uses that name. `fedora-logos` is swapped for `generic-logos`, which is Fedora's published way for a derivative to drop Fedora's marks. A lawyer has not reviewed that guidance.
  - Terminal color schemes under `/usr/share/darkjuander/terminal/`. No terminal in the image is configured to load them.
  - OFL fonts and their license files under `/usr/share/fonts/darkjuander/`, including an Inter Regular instance for Plymouth.
  - `/usr/lib/os-release`: `NAME="DARKJUANDER"`, `PRETTY_NAME="DARKJUANDER"`, `LOGO=darkjuander`, `HOME_URL`, `SUPPORT_URL`, and `BUG_REPORT_URL` set to `https://github.com/Awkward8787/DARKJUANDER`, `ANSI_COLOR` set to violet `#9D8CFF`, `DEFAULT_HOSTNAME=darkjuander`. `ID` stays `fedora`. `ID_LIKE` is not added. `VERSION_ID` and `VARIANT_ID` stay as the base set them. `CPE_NAME`, `DOCUMENTATION_URL`, and the `REDHAT_*` fields stay Fedora's.
  - `/usr/lib/issue` is left as the package shipped it. It already prints `\S`, which is `PRETTY_NAME`, so the image does not add a second DARKJUANDER line in `issue.d`. The brand-guide line "Welcome back. Your agents are where you left them." is planned and is not installed. The status motd is planned and is not installed.

What it does not do:

- It does not sign the image.
- It does not enroll Secure Boot or seal a LUKS key to a TPM.
- It does not load `image/selinux/darkjuander.te`.
- `.github/workflows/release.yml` builds this image on a `v*` tag and on a dry run. A dry run does not publish a release. A hardware boot is not claimed here.
- CI builds the Containerfile and checks the initramfs. That job does not run `bootc-image-builder`, and it does not boot the result.
- The Plymouth theme and the wallpaper have not been seen on a boot. The GRUB artwork is not installed. The pre-login text is the package `/usr/lib/issue`, which prints `PRETTY_NAME` once.

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
