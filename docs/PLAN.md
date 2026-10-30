# Plan for this PR

Phase 0 plus the smallest Phase 1 slice that runs the five UMBRA scenes.
The PRD in `docs/PRD.md` stays the source of truth. This file says what we
reused, what we wrote, and what we are not claiming.

## Reused

| Piece | What we use it for | Why we did not rewrite it |
|---|---|---|
| Fedora bootc 45, mirrored at `ghcr.io/awkward8787/fedora-bootc` by the digest in `image/fedora-bootc.digest` | `dj-base` image | Read-only root and A/B updates already exist. The index includes aarch64 and x86_64. The build uses that digest, not the Quay tag. |
| systemd | `umbrad`, slices, sysusers, tmpfiles | Service lifecycle and resource accounting are already there. |
| nftables | Default-deny inbound | A short ruleset, not a new firewall. |
| Fedora SELinux | Enforcing mode from the base image | A custom domain is a draft in `image/selinux/` and is not loaded. |
| bubblewrap | Agent mount, pid, net, user, ipc, uts, and cgroup namespaces | It is the maintained unprivileged sandbox tool. License LGPL-2.0-or-later, used as a program, not linked. |
| Landlock (`landlock` crate) | File rules inside the confined child | The kernel feature and the crate already exist. |
| llama.cpp v0.8.0 | Optional GGUF engine behind an HTTP adapter | The server already speaks an OpenAI subset. We do not vendor it. |
| PSI (`/proc/pressure`) | Router input | No BPF program is required for the MVP. |
| `sha2` | Audit hash chain | SHA-256 does not need a new implementation. |

## New code

Each of these is glue the reused pieces do not provide together.

| Crate | Why it is unavoidable |
|---|---|
| `dj-label` | Integrity and confidentiality join, including "web and email origins stay untrusted". That rule is the product. |
| `dj-policy` | Four YAML rules with first-match `allow` / `deny` / `approve`. OPA and Cedar can replace the evaluator later. Pulling either in for this rule set adds a second policy runtime before the rules have settled. |
| `dj-audit` | Append-only hash chain and `dj audit verify`. The event shape is ours. |
| `dj-broker` | Grants, then policy, then a fail-closed approval record. There is no trusted desktop dialog yet, so `approve` is logged and denied. |
| `dj-sandbox` | Builds the bubblewrap command and applies Landlock plus a seccomp denylist. Missing `bwrap` refuses to run. |
| `umbra` | One registry record per model, a cache key that includes the trust domain, and a router that drops candidates before it scores. |
| `dj-gateway` | Unix-socket HTTP. Axum and hyper are the HTTP stack. The routes are the API. |
| `umbrad`, `dj` | Process entry points. |

The mock engine is part of `umbra` so CI can run the scenes without weights. It is a simulator that emits `email.send` when it sees `DJ_INJECT_EXFIL`. It is not a language model.

## Layout change from PRD 7.12

`services/umbra` is a library and `services/umbrad` is the binary. The binary has to depend on the gateway, and the gateway has to depend on the library. One crate cannot do both without a cycle. `services/gateway` is the HTTP crate the binary embeds. A separate gateway process is later work.

`bpf/` contains a README and no programs.

## Kernel config check, not a boot

On 2026-10-28 the Fedora 45 dist-git configs were fetched and read with `scripts/check-kernel-config.sh` logic:

- aarch64: `https://src.fedoraproject.org/rpms/kernel/raw/f45/f/kernel-aarch64-fedora.config`
- x86_64: `https://src.fedoraproject.org/rpms/kernel/raw/f45/f/kernel-x86_64-fedora.config`

Both files set `CONFIG_PSI`, `CONFIG_CGROUPS`, `CONFIG_MEMCG`, `CONFIG_BPF_SYSCALL`, `CONFIG_DEBUG_INFO_BTF`, `CONFIG_SCHED_CLASS_EXT`, `CONFIG_SECURITY_LANDLOCK`, `CONFIG_SECURITY_SELINUX`, and `CONFIG_BPF_UNPRIV_DEFAULT_OFF` to `y`. There is no separate `CONFIG_BPF=` line; `CONFIG_BPF_SYSCALL=y` selects BPF.

That is a reading of Fedora's config files. It is not a boot of `dj-base`, and it is not a measurement of PSI inside UTM.

## What this PR does not claim

- The Containerfile has not been built here, and no disk image has been booted.
- Images are not signed. Secure Boot and TPM-sealed LUKS are not configured.
- Audit records are mode `0600` and hash-chained. They are not encrypted.
- The prefix cache records hits and misses. It does not skip token generation, so the time-to-first-token target is not measured.
- `resident_copies: 1` is the registry count for one shared engine object. RSS was not compared with three llama-server processes.
- Per-request cgroup charging is not implemented. `umbrad.service` is placed in `umbra-weights.slice`.
- The learned scorer, expert pager, personal memory index, Anthropic API, and streaming are not implemented.
- No outside security review has been done.
