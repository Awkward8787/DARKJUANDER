# DARKJUANDER __VERSION__

Downloadable disk images and the matching bootc container. The container tag
is `ghcr.io/awkward8787/darkjuander:__VERSION__` when this file is attached to
a GitHub Release. A workflow_dispatch or pull-request run is a dry run: it
uploads workflow artifacts and does not publish a release or move `:latest`.

## What was not tested

- This image has not been booted on a physical Apple silicon Mac or in UTM
  by the maintainers. A QEMU smoke test, when the runner produced a qcow2,
  is a serial-console boot of that disk. It is not a UTM install.
- Secure Boot is not enrolled. The disk is not signed. LUKS is not sealed
  to a TPM.
- The publish job signs `SHA256SUMS` and
  `ghcr.io/awkward8787/darkjuander:__VERSION__` with keyless cosign.
  `bootc switch` does not enforce that signature unless a policy says so.
  Do not turn signature checking off on a machine you depend on unless you
  accept that.
- The console user `dj` starts from the published password `darkjuander`.
  It is not a secret, and first boot expires it. The first console login
  must set a new password. `sshd` is disabled and inbound connections are
  dropped.
- The qcow2 and `ghcr.io/awkward8787/darkjuander` leave Fedora's dnf
  repositories disabled. Update those systems with `bootc`, not `dnf update`.
- The Anaconda ISO is the smoke-tested container plus a depsolve layer.
  The kickstart disables Fedora dnf repos on the installed system. The ISO
  installer was not booted in CI. It is an untested installer. Packages
  installed later with `dnf` can still conflict with `bootc switch`.
- Runner smoke result for this build:

```
__SMOKE__
```

## Verify the download

Check `SHA256SUMS` before you concatenate or decompress. Files larger than
1900 MiB are shipped as `name.part00`, `name.part01`, and so on.

```bash
sha256sum -c SHA256SUMS
cat darkjuander-VERSION-aarch64.qcow2.zst.part* > darkjuander-VERSION-aarch64.qcow2.zst
zstd -d darkjuander-VERSION-aarch64.qcow2.zst
```

Skip the `cat` line when the archive was not split. If
`SHA256SUMS.cosign.bundle` is attached:

```bash
cosign verify-blob \
  --bundle SHA256SUMS.cosign.bundle \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  --certificate-identity-regexp '^https://github.com/Awkward8787/DARKJUANDER/.github/workflows/release.yml@' \
  SHA256SUMS
```

If the bundle is absent, keyless signing did not run. The checksum file is
then only as trustworthy as the GitHub Release that hosts it.

The container tag is signed when this file is attached to a GitHub Release:

```bash
cosign verify \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  --certificate-identity "https://github.com/Awkward8787/DARKJUANDER/.github/workflows/release.yml@refs/tags/__VERSION__" \
  ghcr.io/awkward8787/darkjuander:__VERSION__
```

## Install

Follow https://github.com/Awkward8787/DARKJUANDER/blob/__VERSION__/docs/INSTALL.md at this tag.
The primary disk is the aarch64 qcow2, for UTM on Apple silicon.

## Known limits

The list below is `docs/ROADMAP.md` from the tagged commit. It is the
maintainer's list of what this release does not do.

__ROADMAP__
