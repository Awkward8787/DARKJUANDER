# Install DARKJUANDER

This is the download path. Building the image yourself is still
`scripts/build-image.sh` and `scripts/build-disk.sh`, and those scripts do
not run on macOS.

No step below has been carried out on an M1 Mac. The release workflow can
boot the qcow2 under QEMU on the runner. That is not a UTM install, and it
is not a boot on Apple hardware. If a command fails, stop.

A GitHub Release exists only after a `vX.Y.Z` tag is pushed. A dry run of
the workflow uploads artifacts on the Actions run and does not create that
Release.

## What you are installing

- `darkjuander-<version>-aarch64.qcow2.zst` is the primary disk. UTM on
  Apple silicon virtualizes it. It is already installed; it is not an
  installer ISO.
- `darkjuander-<version>-aarch64.iso.zst` is an Anaconda installer ISO for
  the same architecture.
- `x86_64` files are the same kinds of image when that runner succeeded.
- `darkjuander-binaries-<version>-<arch>.tar.gz` is `dj`, `umbrad`, and
  `dj-auditd` built for that Linux architecture. Those binaries do not run
  on macOS. The disk image is what you boot.
- The console user is `dj`. The initial password is `darkjuander`. It is
  published in `image/console-password.txt`. It is not a secret. The first
  boot expires it (`chage -d 0`), so the first console login must set a new
  password before the shell starts. `sshd` is disabled and the guest
  nftables rules drop inbound connections.
- Checksums are in `SHA256SUMS`. A `SHA256SUMS.cosign.bundle` is a keyless
  signature of that file when the publish job could obtain a GitHub OIDC
  token. The GHCR image is signed the same way on a tag. `bootc` does not
  enforce that cosign signature unless you configure a policy that does.
- The qcow2 and the GHCR container leave Fedora's dnf repositories disabled,
  which is how Fedora bootc ships them. Update that system with `bootc
  switch` or `bootc upgrade`, not `dnf update`.
- The Anaconda ISO is built from the same container image id that produced
  the smoke-tested qcow2, plus one layer that enables Fedora dnf repos so
  the installer can depsolve. A kickstart `%post` turns those repos off
  again on the installed system. The ISO installer itself is not booted in
  CI. Treat it as an untested installer. `bootc` remains the update path.

## 1. Download and check

On the Mac, from the GitHub Release for the tag you intend to boot:

```bash
curl -fLO https://github.com/Awkward8787/DARKJUANDER/releases/download/vX.Y.Z/SHA256SUMS
# Repeat curl -fLO for each file named in SHA256SUMS, including any .partNN files
# and SHA256SUMS.cosign.bundle when the release has one.
sha256sum -c SHA256SUMS
```

macOS does not ship `sha256sum`. Use `shasum -a 256 -c SHA256SUMS`.

If the qcow2 was split:

```bash
cat darkjuander-vX.Y.Z-aarch64.qcow2.zst.part* > darkjuander-vX.Y.Z-aarch64.qcow2.zst
```

Decompress:

```bash
zstd -d darkjuander-vX.Y.Z-aarch64.qcow2.zst
```

`zstd` on macOS is `brew install zstd` when it is not already installed.
The checksum covers the compressed files you downloaded, not the
decompressed qcow2.

When the release includes a cosign bundle:

```bash
cosign verify-blob \
  --bundle SHA256SUMS.cosign.bundle \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  --certificate-identity-regexp '^https://github.com/Awkward8787/DARKJUANDER/.github/workflows/release.yml@' \
  SHA256SUMS
```

A failed verify means you should not boot that download. A missing bundle
means the checksum file was not signed. The release notes for that tag say
which of the two it is.

## 2. Create the UTM VM on an M1 Mac

1. Install UTM. Create a new VM. Choose Virtualize, not Emulate. Operating
   system: Linux. Architecture: ARM64.
2. Give it 4 CPU cores and 8192 MB of memory to boot the mock engine. Use
   8 cores and 32768 MB if you later load a local model. Inference in the
   guest is CPU-only.
3. Delete the empty drive UTM created. Add the decompressed qcow2 as a
   VirtIO drive. Boot with UEFI. Do not enable Secure Boot. Apple silicon
   does not give the guest a TPM.
4. Do not expect the first session over SSH. The guest drops inbound
   connections, and `sshd` is disabled.
5. Start the VM and open the UTM console.

## 3. First login

At the console:

```text
login: dj
Password: darkjuander
```

Then:

```bash
passwd
```

Set a password that is not the published default.

`umbrad` is enabled and uses the mock engine. It listens on
`/run/darkjuander/umbrad.sock`, which is not world-accessible. Cloud
routing is off. The unit still denies non-localhost egress.

Check the units:

```bash
systemctl status dj-auditd umbrad --no-pager
```

## 4. `dj provider add`

Provider ids that exist today are `openai` and `xai`. Adding a key does not
grant the provider and does not open egress. The key is read from a file or
from stdin. Do not pass it as a flag.

```bash
umask 077
printf '%s' 'paste-the-key-here' > "${HOME}/openai.key"
sudo dj provider add openai --model gpt-4o --key-file "${HOME}/openai.key"
shred -u "${HOME}/openai.key" || rm -f "${HOME}/openai.key"
sudo dj provider list
```

`granted` stays false. `sudo dj provider grant openai` is a separate
decision. The image unit still blocks egress after a grant. Read
[providers.md](providers.md) before you change that.

## 5. Anaconda ISO

The ISO is the installer, not a preinstalled disk. In UTM, create an ARM64
Linux VM, attach `darkjuander-<version>-aarch64.iso` as a USB or CD drive,
and add an empty VirtIO disk of at least 16 GB. Boot the ISO, accept the
install onto the empty disk, shut down, remove the ISO, and boot the disk.
The installed system is the smoke-tested container with Fedora dnf repos
turned back off by the kickstart. User `dj` must set a new password at the
first console login. There is no SSH. This ISO has not been booted in CI
and has not been walked through on hardware.

## 6. `bootc switch`

On a machine that is already Fedora bootc, and only if you accept an
unsigned image:

```bash
sudo bootc switch ghcr.io/awkward8787/darkjuander:vX.Y.Z
sudo systemctl reboot
```

`:latest` moves on every tagged release. Pin the version tag. The container
is pushed to GHCR only when a tag is built, not on a dry run. The package
may be private until it is made public; a private package cannot be pulled
by `bootc switch`. The disk image's console user is applied by
bootc-image-builder and is not part of the container, so `bootc switch` does
not create user `dj`.

A tag publish signs the GHCR image with keyless cosign. `bootc switch` uses
bootc's own policy, which does not check that cosign signature unless you
configure it to. Do not disable signature policy to get past a refusal
unless that is a decision you mean to make.

```bash
cosign verify \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  --certificate-identity "https://github.com/Awkward8787/DARKJUANDER/.github/workflows/release.yml@refs/tags/vX.Y.Z" \
  ghcr.io/awkward8787/darkjuander:vX.Y.Z
```
