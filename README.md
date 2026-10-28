# DARKJUANDER

DARKJUANDER is a Linux system for running AI agents with a boundary around
them. The working name belongs to Anthony Jaamal Jones (Jaamal), Vibe Virtue
Enterprise LLC. It is not a new kernel. It is Fedora bootc plus a userspace
neural core called UMBRA.

The product requirements are in [docs/PRD.md](docs/PRD.md). The diagrams are
[docs/diagrams/architecture.mmd](docs/diagrams/architecture.mmd) and
[docs/diagrams/umbra-dataflow.mmd](docs/diagrams/umbra-dataflow.mmd).

## What the demos do not prove

`dj demo` and `cargo test` run the mock engine on the host that builds the
tests. Passing them does not mean DARKJUANDER enforces the PRD threat model.

- A private file read through the broker returns a span, not the bytes, and
  that call raises the caller's session floor. Resubmitting those bytes in the
  same session with a public label stays local. An agent that copies the bytes
  through some other channel, or into a different session that never touched
  the file, is not caught. Content inspection (PRD PE-6 and PE-7) is not built.
- The audit log detects truncation and field swaps against its signed
  checkpoint. `dj-auditd` holds the signing seed as user `dj-audit`. `umbrad`
  only has the socket and the published verifying key at
  `/var/lib/dj-audit/audit.ed25519.pub`. A compromised `umbrad` cannot make the
  signer rewind a head. This still is not a TPM: root, or anyone who gets
  `dj-audit`'s uid, can sign. The log is not encrypted.
- `workspace = "/"` is rejected. The seccomp profile is still a denylist, not
  an allowlist. cgroup limits and the SELinux domain are not enforced by these
  tests.
- No disk image has been built or booted. Section 7.9 numbers are targets.
- `allow_psi_override` is false in `examples/config/umbrad.yaml` and must stay
  false outside tests. The demo turns it on only in a temporary config so
  scene 4 can cross the PSI threshold without filling the machine's RAM.

## What works today

On a Linux machine, `cargo test` and `dj demo` run five scenes against a mock
engine. No weights are required. Read the section above before treating a pass
as a security boundary.

1. Two agents call `mid-local`. The registry shows one resident copy and both callers.
2. Agent A hits its own prefix cache. Agent B, in another trust domain, misses. The miss response does not name Agent A's domain.
3. A test page with a hidden `email.send` instruction is labeled untrusted. The broker records an approval prompt and then denies the send. `dj audit verify` accepts the log.
4. When PSI `memory some avg10` crosses the threshold, new `default-chat` requests move from `mid-local` to `small-local`. The audit reason contains `PSI memory some avg10`.
5. A private file read returns a span handle and no bytes. The same agent then resubmits those bytes with a public label. The session floor stays private, the scorer still prefers the cloud model, and the hard rule keeps the call on `mid-local`. The cloud engine's call count stays 0. The private bytes are not written to the audit log.

`umbrad` serves a Unix socket. `GET /v1/models` and `POST /v1/chat/completions` are the OpenAI-shaped routes. Tool calls go to `POST /v1/dj/tool`. `GET /v1/dj/top` is the admin view.

`dj sandbox plan` prints a bubblewrap command that drops the network and capabilities. `dj sandbox run` refuses to start if `bwrap` is missing. When `bwrap` is present, the child applies Landlock and a seccomp denylist before `exec`.

The image definition is [image/Containerfile](image/Containerfile). It targets Fedora bootc 45 for aarch64 and x86_64.

## What is planned

The honest list is [docs/ROADMAP.md](docs/ROADMAP.md). In short: the disk image has not been built or booted, images are not signed, the audit log is not encrypted, cache hits do not skip inference, there is no desktop and no trusted approval dialog, and there is no real cloud provider client. The Section 7.9 numbers are targets, not measurements. An in-process check that routing returns in under 5 ms is not the guest p95 target.

## Build and test the userspace

Rust 1.99.1 is pinned in `rust-toolchain.toml`.

```bash
cargo test --workspace --locked
./scripts/run-demos.sh
dj audit verify /path/to/audit.log
```

`cargo test` includes the five scenes. `./scripts/run-demos.sh` prints them.

Optional real engine, still not run in CI:

```bash
./scripts/fetch-model.sh
./scripts/build-llama.sh
```

The GGUF checksum and the llama.cpp commit are in [engines/llama/README.md](engines/llama/README.md).

## Boot on an M1 Max with UTM

This repository has not produced a disk image, and nobody has booted one in UTM yet. The steps below are the build path. Stop if a command fails. Do not treat a failed image build as a bootable DARKJUANDER.

You need Linux to build the image. `cargo build` on macOS produces a macOS binary, and the guest cannot run it. On the Mac, use UTM to boot a normal Fedora 45 aarch64 VM first (Fedora's own cloud or raw image, not this repo), give it 8 CPUs and about 32 GB of RAM, and build inside that VM. Podman machine on macOS is a poor fit for `bootc-image-builder`, which needs privileged loop devices.

Inside that Linux aarch64 VM:

```bash
sudo dnf install -y git podman rustc cargo cmake gcc gcc-c++ make
git clone https://github.com/Awkward8787/DARKJUANDER.git
cd DARKJUANDER
cargo test --workspace --locked
./scripts/run-demos.sh
./scripts/build-image.sh
./scripts/build-disk.sh "$HOME/.ssh/authorized_keys"
```

`build-disk.sh` writes raw and qcow2 files under `out/disk/` when bootc-image-builder succeeds. Copy the raw image back to the Mac.

In UTM on the M1 Max:

1. Create a new VM. Choose Virtualize, not Emulate. Operating system: Linux. Architecture: ARM64.
2. Give it 8 CPU cores and 32768 MB of memory. Inference in the guest is CPU-only.
3. Use the raw disk as the boot drive. Boot with UEFI. Do not expect a TPM or Secure Boot chain. Apple silicon does not provide one to the guest.
4. Open the UTM console. The guest nftables rules drop inbound SSH, so the first session is the console, not SSH.
5. Log in with the root key only after you have added an SSH accept rule yourself. The build does not set a password.
6. Check the running kernel, which may differ from the Fedora 45 dist-git file already read:

```bash
sudo ./scripts/check-kernel-config.sh
```

7. The default `umbrad` uses the mock engine and is local-only. To try SmolLM2:

```bash
./scripts/fetch-model.sh
./scripts/build-llama.sh /usr/local/src/llama.cpp /usr/local
sudo install -D -m 0755 /usr/local/bin/llama-server /usr/libexec/darkjuander/llama-server
sudo install -D -m 0640 "$HOME/.local/share/darkjuander/models/SmolLM2-135M-Q4_K_M.gguf" /var/lib/darkjuander/models/default.gguf
sudo systemctl enable --now dj-llama.service
```

Then edit `/etc/darkjuander/umbrad.yaml` so `mid-local` uses `kind: llama-cpp` and `base_url: http://127.0.0.1:9931`, and restart `umbrad`. That edit is not done for you.

`stress-ng` is in the image. `allow_psi_override` is false there, so pressure has to come from `/proc/pressure/memory`. A memory hog is `stress-ng --vm 2 --vm-bytes 80%`. Whether the router switches within two seconds has not been measured.

## QEMU on x86_64 Linux

Build with the same scripts on an x86_64 Linux host. `scripts/build-image.sh` selects `linux/amd64` from `uname -m`.

After `out/disk/` contains a qcow2 or raw image, boot it with the OVMF firmware your distro ships. The path differs. On Fedora it is often `/usr/share/edk2/ovmf/OVMF_CODE.fd`.

```bash
qemu-system-x86_64 \
  -machine q35,accel=kvm \
  -cpu host \
  -m 8192 -smp 4 \
  -drive if=pflash,format=raw,readonly=on,file=/usr/share/edk2/ovmf/OVMF_CODE.fd \
  -drive file=out/disk/disk.raw,format=raw,if=virtio \
  -nographic -serial mon:stdio
```

The filename under `out/disk/` is whatever bootc-image-builder writes. This command has not been run. Inbound SSH is dropped. Use the serial console.

## Layout

```
image/                 Containerfile, systemd units, nftables, sysctl, draft SELinux
services/label         integrity and confidentiality labels
services/audit         hash-chained log
services/policy        YAML rules
services/broker        grants and mediated reads
services/sandbox       bubblewrap plan, Landlock, seccomp denylist
services/umbra         registry, cache, PSI router, mock and llama adapters
services/gateway       Unix-socket API and the five scenes
services/umbrad        umbrad binary
shell/dj               dj binary
engines/llama          upstream pin and checksum
bpf/                   empty of programs, on purpose
policy/default.yaml    shipped rules
tests/injection        the exfil page the mock treats as an instruction
```

`docs/PLAN.md` explains why the library and the binary are split.

## License

Apache-2.0 for the code in this repository. Other licenses are listed in
[docs/LICENSES.md](docs/LICENSES.md). DARKJUANDER and UMBRA have not had a trademark search.
