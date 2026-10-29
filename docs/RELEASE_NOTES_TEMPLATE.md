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
- cosign keyless, when the workflow could get a GitHub OIDC token, signs
  `SHA256SUMS` only. That is not a signature on the bootc image itself.
  `bootc switch` may refuse an unsigned image. Do not turn signature
  checking off on a machine you depend on unless you accept that.
- The console user `dj` uses the published password `darkjuander`. It is not
  a secret. `sshd` is disabled and inbound connections are dropped. Change
  the password with `passwd` before you add a network path you do not control.
- The qcow2 and `ghcr.io/awkward8787/darkjuander` leave Fedora's dnf
  repositories disabled. Update those systems with `bootc`, not `dnf update`.
- The Anaconda ISO is built from a throwaway image that enables those
  repositories so the installer can depsolve. A system installed from the
  ISO has the Fedora repositories enabled. `dnf` can then see Fedora
  mirrors. That is not the supported update path. Packages installed with
  `dnf` can conflict with a later `bootc switch` or `bootc upgrade`.
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

## Install

Follow https://github.com/Awkward8787/DARKJUANDER/blob/__VERSION__/docs/INSTALL.md at this tag.
The primary disk is the aarch64 qcow2, for UTM on Apple silicon.

## Known limits

The list below is `docs/ROADMAP.md` from the tagged commit. It is the
maintainer's list of what this release does not do.

__ROADMAP__
