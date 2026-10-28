# Roadmap

Follow-ups that this PR deliberately did not build. Priority names are the PRD's.

## Still inside Phase 0

- Build `dj-base` with `scripts/build-image.sh` and `scripts/build-disk.sh` on an aarch64 Linux host and boot it in UTM. Record the console and `scripts/check-kernel-config.sh` from inside the guest.
- Sign the image (BASE-4). cosign is the planned tool. Nothing is signed now.
- Compile and load an SELinux domain for `umbrad` and for agents (BASE-5, SB-4). The draft in `image/selinux/` is not that domain.

## Phase 1, P0 not finished

| ID | Left to do |
|---|---|
| BASE-1 | CI does not build the bootc image. The workflow only lints the Containerfile. |
| BASE-3, UP-1, UP-2 | A/B updates and rollback come from bootc and have not been tested on a disk. |
| BASE-9 | No qcow2 or raw artifact is produced here. |
| RT-1, RT-2 | llama-server is an adapter and a disabled unit. It is not confined beyond the unit file, and it has not been run against the fetched GGUF in CI. |
| RT-4 | The router does not refuse a load based on a live RAM estimate. |
| GW-1 | Chat completions and the models list exist. Streaming and tool-call streaming do not. |
| GW-2 | No Anthropic messages endpoint. |
| GW-6 | No keyring. There is no cloud client, so there is no stored provider key. |
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

- Cloud providers. The router can drop a cloud candidate. There is no OpenAI, Anthropic, or other client.
- Jaamal's real agents are not onboarded.
- No outside review of the sandbox and broker.

## Later

UM-10 through UM-19, the P1 rows in sections 8.2 through 8.10, and phases 2 through 4 stay as written in the PRD. `umbra-memory` stays off until someone opts in. `scx_umbra` stays unwritten. The Fedora 45 kernel config does set `CONFIG_SCHED_CLASS_EXT=y`, which removes one unknown and does not by itself justify writing the scheduler.
