# Roadmap

Follow-ups that this PR deliberately did not build. Priority names are the PRD's.

## Still inside Phase 0

- Boot a built `dj-base` disk in UTM on Apple silicon and record the console and `scripts/check-kernel-config.sh` from inside the guest. The release workflow can build the disk. That is not this boot.
- Sign the bootc image for `bootc` policy (BASE-4). The release workflow can cosign-sign `SHA256SUMS` and, on a tag, the GHCR index. The disk image itself is not signed.
- Compile and load an SELinux domain for `umbrad` and for agents (BASE-5, SB-4). The draft in `image/selinux/` is not that domain.
- Boot the image and look at the Plymouth theme and the wallpaper. CI builds the Containerfile and runs `image/check-initramfs.sh`, which requires the theme script path and `Theme=darkjuander` inside the initramfs. That is not a boot. GRUB artwork is not installed, because bootupd's static config does not load a theme. The brand guide's login greeting and status motd stay planned and are not hard-coded.

## Phase 1, P0 not finished

| ID | Left to do |
|---|---|
| BASE-1 | CI builds the Containerfile with stub binaries and checks the initramfs. `.github/workflows/release.yml` builds the release image and disk. A green dry run is a runner log, not a hardware boot. |
| BASE-3, UP-1, UP-2 | A/B updates and rollback come from bootc and have not been tested on a disk. |
| BASE-9 | The release workflow produces a qcow2 (`image` job) and an Anaconda ISO built from that smoke-tested image. The ISO installer is not booted in CI. Both are attached to a GitHub Release only when a `v*` tag is pushed; publish needs the qcow2 image job, not the ISO. |
| RT-1, RT-2 | llama-server is an adapter and a disabled unit. It is not confined beyond the unit file, and it has not been run against the fetched GGUF in CI. |
| RT-4 | The router does not refuse a load based on a live RAM estimate. |
| GW-1 | Chat completions and the models list exist. Streaming and tool-call streaming do not. |
| GW-2 | No Anthropic messages endpoint. |
| GW-6 | API keys are owner-only files or systemd `LoadCredential` entries. There is no kernel keyring. Cloud routing stays off until a provider is granted. See docs/providers.md. |
| SB-2 | The child seccomp profile is still a denylist (`dj_sandbox::DENIED_SYSCALLS`). An allowlist is future work; a short allowlist would break ordinary agent programs, so this slice does not pretend to have one. CI does not execute the filter. |
| PE-6, PE-7 | No content inspection. Session floors stop the broker-read laundering path. They do not stop a copied byte string that arrives through another channel. |
| SB-5 | cgroup limits are attempted and reported. They are not applied unless the host cgroup tree accepts the write. |
| SB-8 | No kill switch UI. Stopping the process is left to the operator. |
| PB-4, PB-6, UI-3 | Approval is fail-closed. There is no trusted dialog. |
| AU-3 | The log is not encrypted. |
| AU-8 | The audit seed is not sealed to a TPM, and offline verify has no external anchor, so restoring an old log together with its old checkpoint still verifies. `dj-auditd` keeps the seed under a separate uid and signs only a record that extends the head it already signed. A TPM-backed counter is the next step. |
| UM-2 | Shared residency is one engine object in-process. llama.cpp is not yet the shared process, and RSS is unmeasured. |
| UM-5 | Cache keys partition by trust domain. Hits do not restore a llama.cpp slot, so the TTFT target is unmeasured. |
| UM-7 | PSI fallback works against a pressure file or the demo override. It has not been timed against `stress-ng` in a guest. The image default leaves the override off. |
| UM-8 | The daemon's systemd unit uses `umbra-weights.slice`. Inference is not moved into the caller's cgroup per request. |
| UI-1 through UI-5 | No desktop and no control center. `dj` covers audit verify, sandbox plan/run, and the demo. |

## Phase 1 exit, still open

- Anthropic is not a client. xAI and OpenAI share one OpenAI-compatible adapter; see docs/providers.md. The image unit still blocks egress.
- Jaamal's real agents are not onboarded.
- No outside review of the sandbox and broker.

## Later

UM-10 through UM-19, the P1 rows in sections 8.2 through 8.10, and phases 2 through 4 stay as written in the PRD. `umbra-memory` stays off until someone opts in. `scx_umbra` stays unwritten. The Fedora 45 kernel config does set `CONFIG_SCHED_CLASS_EXT=y`, which removes one unknown and does not by itself justify writing the scheduler.
