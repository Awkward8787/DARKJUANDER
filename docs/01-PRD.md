# DARKJUANDER: Product Requirements Document

| Field | Value |
| --- | --- |
| Project | DARKJUANDER |
| Document | Product Requirements Document |
| Document ID | DJR-PRD |
| Version | 1.2 |
| Status | In build (Phase 0: v0.1.1 published 2026-10-30 as a signed GitHub Release and GHCR image; qcow2 QEMU-smoke-tested on CI runners; never booted in UTM or on Apple hardware) |
| Date last updated | 2026-10-30 (America/New_York, EDT) |
| Author | Grok Bot documentation agent, for owner Jaamal (Anthony Jaamal Jones) |
| Repo | https://github.com/Awkward8787/DARKJUANDER (public, created 2026-10-27 11:37 PM EDT; default branch `main`) |
| Commit described | `ba652a285a` (full `ba652a285aac4a281f807bdb65570cf4c5c005d0`; 2026-10-30 10:58 AM EDT, PR #15 "Mirror the Fedora bootc 45 pin into GHCR"; tag `v0.1.1`). Previous: `9660261cbc` (PR #14) |
| Live URL | No hosted site. Release [v0.1.1](https://github.com/Awkward8787/DARKJUANDER/releases/tag/v0.1.1); container `ghcr.io/awkward8787/darkjuander:v0.1.1` (= `:latest`, index `sha256:02f054ac30b3…`, cosign-signed) |

## Table of contents

1. [Summary](#1-summary)
2. [Background and problem](#2-background-and-problem)
3. [Goals and non-goals](#3-goals-and-non-goals)
4. [Success metrics](#4-success-metrics)
5. [Audience and personas](#5-audience-and-personas)
6. [User stories](#6-user-stories)
7. [Functional requirements](#7-functional-requirements)
8. [Non-functional requirements](#8-non-functional-requirements)
9. [Content requirements](#9-content-requirements)
10. [Monetization or payments](#10-monetization-or-payments)
11. [Scope by release](#11-scope-by-release)
12. [Assumptions and constraints](#12-assumptions-and-constraints)
13. [Risks](#13-risks)
14. [Stakeholders and approvals](#14-stakeholders-and-approvals)
15. [Glossary](#15-glossary)
16. [Open questions](#16-open-questions)
17. [Changelog](#17-changelog)
18. [Sources](#18-sources)

## 1. Summary

DARKJUANDER is a Linux system for running AI agents inside a boundary: a hardened, image-based Fedora bootc 45 OS (`dj-base`) plus a Rust userspace (UMBRA neural core, permission broker, policy engine, sandbox, signed hash-chained audit log, and the `dj` CLI). It is model-agnostic: an OpenAI-shaped API on a local Unix socket routes each request to local engines (mock or llama.cpp) or, only when explicitly granted, to xAI/OpenAI cloud models, using security labels and live memory pressure. It is not a new kernel. As of `main` @ `9660261c` (2026-10-29) the userspace builds, tests and runs five demo scenes against a mock engine; a tag-triggered release workflow builds the bootc image, qcow2 and Anaconda ISO for aarch64 and x86_64 and boots the qcow2 under QEMU on the CI runner; the image carries the Umbra Eclipse brand (Plymouth splash, wallpapers, icons, os-release). The first tag, v0.1.0, failed at image signing and left an unsigned container public on GHCR. On 2026-10-30 the base image moved to a GHCR mirror pinned by digest (PR #15) and v0.1.1 was published as the first signed release (https://github.com/Awkward8787/DARKJUANDER/releases/tag/v0.1.1): qcow2, ISO and binaries for both arches, a cosign-signed `SHA256SUMS`, and a cosign-signed GHCR image that `:latest` now points to. v0.1.0 stays published, unsigned, and is replaced by v0.1.1. Nothing has been booted in UTM or on Apple hardware, and the README still says passing the demos does not prove the threat model.

## 2. Background and problem

From PRD v0.2 (2026-10-27): Jaamal wants "my own custom operating system designed to be completely flexible with custom AI models as a part of its infrastructure," which "needs to be Linux base being that it's designed to be for AI Security." Agents on a normal desktop get either too much access (SSH keys, cookies, all files) or too little; prompt injection (OWASP LLM01) cannot be fixed at the model level; models change monthly; there is no shared record of what several agents did; and local models are hard to run well. Why now: practical local inference, de-facto OpenAI/Anthropic API shapes, MCP tool calls, unprivileged kernel sandboxing (Landlock, namespaces, seccomp) and mature image-based Linux (bootc).

## 3. Goals and non-goals

**Goals (v1, from PRD 4.1)**
1. Hardened image-based Linux with atomic A/B updates and rollback (FR-001).
2. System AI API (OpenAI- and Anthropic-compatible) on localhost routing to user-chosen local or cloud models (FR-004, FR-005).
3. Install, verify, swap and remove models without breaking apps (FR-006).
4. Each agent in a sandbox with declared, user-granted permissions (FR-008).
5. User approval for destructive and outbound actions (FR-009).
6. Tamper-evident local audit log of every agent action (FR-010).
7. No telemetry; every AI-layer egress visible (NFR-004).
8. VM image first, then real hardware (FR-002).
9. UMBRA: shared model residency, label-aware cache and routing, pressure-aware fallback (FR-006, FR-007).

**Non-goals (v1)**: no new kernel; not a phone/tablet OS; no in-house foundation model; no verified microkernel in the MVP; no Windows/macOS compatibility layer; no enterprise fleet management; no always-on screen recording or "remember everything" feature.

## 4. Success metrics

All PRD numbers are targets, not data (PRD section 12, UMBRA section 7.9), for example: zero sandbox escapes unfixed more than 14 days; 100% of agent tool and model calls in the audit log during test runs; 100% of tainted outbound/destructive actions reach an approval prompt in the injection suite; critical upstream fixes shipped within 7 days; boot to first local response in under 15 minutes; zero connections to DARKJUANDER servers in local-only mode. Measured today: `cargo test --workspace --locked` and the five demo scenes pass in CI (`ci.yml`); no guest-side numbers exist. Adoption targets are unset (Jaamal to set).

## 5. Audience and personas

| Persona | Needs | Status |
| --- | --- | --- |
| P1 Solo founder running an agent fleet (Jaamal is user zero) | Many agents with own keys, files and limits; one audit trail; cost control across local and cloud | Primary |
| P2 Developer | Local model API existing tools can point at; safe sandboxes for coding agents | Primary |
| P3 Creator (music, video, games; e.g. SONA) | Local models without uploading work | Secondary |
| P4 Privacy-minded user | No telemetry, local-first defaults, control over egress | Secondary |
| P5 Security researcher / red-teamer | Controlled policy test platform | Later |

Not v1 targets: mainstream consumers, fleet-managed enterprises, gamers who need anti-cheat.

## 6. User stories

- US-001 As P1, I point two agents at `default-chat` and see one resident model copy serving both (scene 1).
  - Given two principals with `model.use:mid-local`, When both call `/v1/chat/completions`, Then `GET /v1/dj/top` shows one resident copy and both callers.
- US-002 As P1, my agents in different trust domains never share cached prefixes (scene 2).
  - Given agent A cached a prefix, When agent B in another trust domain sends the same prefix, Then B misses and the response does not name A's domain.
- US-003 As P1, a web page with a hidden `email.send` instruction cannot make my agent send mail (scene 3).
  - Given untrusted context, When the agent calls `/v1/dj/tool` with `email.send`, Then the broker records an approval prompt, denies the send, and `dj audit verify` accepts the log.
- US-004 As P1, under memory pressure new requests fall back to a smaller local model (scene 4).
  - Given PSI `memory some avg10` above 25.0, When `default-chat` is requested, Then the route moves from `mid-local` to `small-local` with reason "PSI memory some avg10".
- US-005 As P1, private file bytes never reach a cloud model, even if an agent relabels them public (scene 5).
  - Given a private read returned a span and raised the session floor, When the agent resubmits the bytes labeled public, Then the call stays on `mid-local`, the cloud engine call count stays 0, and the bytes are not written to the audit log.
- US-006 As P1, I add an xAI or OpenAI key from a file, explicitly grant it, test it with a fixed "ping", and revoke it later.

## 7. Functional requirements

| ID | Requirement | Priority | Status | Implemented in |
| --- | --- | --- | --- | --- |
| FR-001 | Hardened `dj-base` image: Fedora bootc 45 pinned by digest in `image/fedora-bootc.digest` and pulled from the GHCR mirror `ghcr.io/awkward8787/fedora-bootc@sha256:479ad7d1…` (PR #15, 2026-10-30; the earlier Quay pin `sha256:0688bb4f…` was pruned by Quay), bubblewrap, nftables default-deny inbound, sysctl hardening, sshd disabled, count-me masked, `umbrad` + `dj-auditd` + `dj-expire-password` enabled, xfs root for bootc install | Must | Built in CI (stub binaries) and in release runs; QEMU smoke on runner; not booted in UTM | `image/Containerfile`, `image/*` |
| FR-002 | Bootable VM disk images (raw, qcow2) for UTM on Apple silicon and QEMU on x86_64 | Must | Shipped: v0.1.1 Release carries `darkjuander-v0.1.1-{aarch64,x86_64}.qcow2.zst` and split `.iso.zst.part00/01`; both qcow2 passed QEMU smoke on the runner; not booted in UTM | `scripts/build-release-disk.sh`, `.github/workflows/release.yml` |
| FR-003 | Labels: integrity and confidentiality join; web and email origins stay untrusted | Must | Shipped | `services/label` |
| FR-004 | Unix-socket API: `GET /health`, `GET /v1/models`, `POST /v1/chat/completions`, `POST /v1/dj/tool`, `GET /v1/dj/top` (admin), `POST /v1/dj/admin/psi` (test hook) | Must | Shipped (no streaming) | `services/gateway/src/lib.rs`, `handlers.rs` |
| FR-005 | Anthropic messages endpoint | Must (PRD GW-2) | Planned | — |
| FR-006 | UMBRA registry (one resident record per model), aliases (`default-chat` → small/mid), trust-domain-partitioned prefix cache | Must | Shipped (cache records hits; does not skip inference) | `services/umbra/src/{registry,cache}.rs` |
| FR-007 | Router: hard rules drop candidates before scoring (private/secret never cloud; caller and daemon `local_only`; grants), then PSI-aware fixed scorer | Must | Shipped | `services/umbra/src/route.rs`, `psi.rs` |
| FR-008 | Agent sandbox: bubblewrap plan (no network, dropped capabilities), Landlock file rules and seccomp denylist in the child; refuses without `bwrap`; rejects `workspace = "/"` | Must | Shipped (denylist, not allowlist; cgroup limits best-effort) | `services/sandbox`, `dj sandbox plan/run` |
| FR-009 | Broker: grants then first-match YAML policy (`allow`/`deny`/`approve`); `approve` is logged and denied until a trusted dialog exists; mediated private reads return span handles | Must | Shipped (fail-closed approvals) | `services/broker`, `services/policy`, `policy/default.yaml` |
| FR-010 | Append-only SHA-256 hash-chained audit log; separate `dj-auditd` signs ed25519 checkpoints only for records extending its last signed head; `dj audit verify` | Must | Shipped (not encrypted; no TPM anchor) | `services/audit` |
| FR-011 | Cloud providers: one OpenAI-compatible adapter with xAI and OpenAI presets; keys via file/stdin or systemd `LoadCredential`; explicit `grant`; host-pinned `dj provider test`; OpenAI Sign in with ChatGPT (PKCE, Responses API) | Should | Shipped, off by default | `services/umbra/src/{provider,credential,siwc}.rs`, `engine/remote.rs`, `shell/dj/src/provider_cmd.rs` |
| FR-012 | Optional llama.cpp v0.8.0 engine (SmolLM2 135M Q4_K_M) behind an HTTP adapter; `dj-llama.service` installed, not enabled | Should | Shipped as adapter; not run in CI | `services/umbra/src/engine/llama.rs`, `scripts/fetch-model.sh`, `build-llama.sh` |
| FR-013 | Five demo scenes runnable with `dj demo` / `scripts/run-demos.sh` against the mock engine | Must | Shipped, run in CI | `services/gateway/src/demo.rs` |
| FR-014 | Signed images (cosign), Secure Boot, TPM-sealed LUKS, loaded SELinux domain | Must (BASE-4/5) | Partial: v0.1.1 `SHA256SUMS` and GHCR index are keyless-cosign-signed (verified 2026-10-30); v0.1.0 image unsigned (kept); disk not signed; bootc does not enforce cosign; Secure Boot/TPM/SELinux Planned | `release.yml`, `docs/ROADMAP.md` |
| FR-015 | Desktop, trusted approval dialog, control center, kill switch | Must (UI-1..5, SB-8) | Planned | — |
| FR-016 | Umbra Eclipse branding wired into the image: Plymouth theme in initramfs, wallpapers, hicolor icon, OFL fonts, terminal schemes, os-release `NAME=DARKJUANDER`; GRUB art kept but not installed | Should | Shipped (#4); splash never seen on a boot | `branding/`, `image/apply-branding.sh`, `image/Containerfile` |
| FR-017 | Tag-triggered release workflow producing qcow2/ISO for x86_64 and aarch64 with QEMU smoke test, `SHA256SUMS`, cosign signing, GHCR push and GitHub Release | Should | Shipped (#9, signing fix #14, mirror #15); v0.1.0 publish failed; v0.1.1 published and signed 2026-10-30 | `.github/workflows/release.yml` |
| FR-018 | Downloadable install path: `docs/INSTALL.md` (verify checksums and cosign bundle, UTM VM, first login with forced password change, `bootc switch`) | Should | Shipped as docs; never walked through on Apple hardware | `docs/INSTALL.md` |
| FR-019 | Brand permission process: proprietary brand license plus GitHub issue template | Should | Shipped | `branding/LICENSE-BRAND.md`, `.github/ISSUE_TEMPLATE/brand-permission.yml` |
| FR-020 | Dependabot weekly updates for GitHub Actions | Could | Shipped; PRs #10-#13 open | `.github/dependabot.yml` |

## 8. Non-functional requirements

- NFR-001 Security: least privilege by default; private/secret data never takes a cloud route by a code rule independent of the policy file; approvals fail closed; secrets never in argv or logs.
- NFR-002 Isolation: `umbrad` runs as `dj-umbra` with systemd sandboxing (`ProtectSystem=strict`, `NoNewPrivileges`, `MemoryDenyWriteExecute`, `SystemCallFilter=@system-service`, `IPAddressAllow=localhost`, `IPAddressDeny=any`).
- NFR-003 Tamper evidence: every record hash-chained and checkpoint-signed by a separate uid; known limit: a full rollback to an old log plus its matching checkpoint still verifies offline.
- NFR-004 Privacy: no telemetry (rpm-ostree count-me masked); local-only default; audit stores provider/model/route/token counts, never keys, tokens or prompt text.
- NFR-005 Honesty: README and PRD distinguish demos from enforcement; Section 7.9 numbers are targets.
- NFR-006 Portability: aarch64 and x86_64; first target is a UTM VM on an M1 Max (8 vCPU, 32 GB) — not yet booted.
- NFR-007 Supply chain: base image pinned by digest and mirrored to GHCR so a Quay prune cannot break builds (PR #15); tag and PR builds only check the mirror, never write it; Rust 1.99.1 pinned; `Cargo.lock` with `--locked`; every GitHub Action pinned by commit SHA with `persist-credentials: false`; release artifacts checksummed and keyless-cosign-signed; no AGPL dependencies; no weights committed.
- NFR-008 Brand integrity: logos and artwork are proprietary (`branding/LICENSE-BRAND.md`), separate from the Apache-2.0 code; no ® (not registered); never imply a security certification.

## 9. Content requirements

Repository docs are the product's public face: `README.md` (what works, what the demos do not prove, build/boot steps), `docs/PRD.md`, `docs/PLAN.md`, `docs/ROADMAP.md`, `docs/BASE.md`, `docs/providers.md`, `docs/LICENSES.md`, `docs/decisions/0001-phase0-defaults.md`. Tone: plain, candid about limits, no performance claims without measurements. New with v1.1: README header with the Umbra Eclipse horizontal logo and tagline **"Agents inside a boundary you can audit."**, `docs/INSTALL.md`, `docs/RELEASE_NOTES_TEMPLATE.md`, `branding/brand-guide.md` (v1.0, October 2026: positioning, five tagline options, voice rules and banned words such as "military-grade" and "unhackable", sample posts), `branding/LICENSE-BRAND.md`.

## 10. Monetization or payments

None. Business model unset (PRD Q3, decision 0001). Cloud usage is billed by each provider to the operator's own account; ChatGPT plan usage is metered by OpenAI.

## 11. Scope by release

| Release | Content | Status |
| --- | --- | --- |
| Phase 0 + Phase 1 slice | FR-003-FR-013 userspace, image definition, CI | Merged 2026-10-28/29 (PRs #1-#3, #5-#8) |
| Phase 0 brand + release (PRs #4, #9, #14) | FR-016-FR-020: Umbra Eclipse in the image, release workflow, INSTALL docs, signing fix | Merged 2026-10-29 |
| v0.1.0 (tag on `44bec1f1`, 2026-10-29 1:47 PM EDT) | First tag publish | Failed at cosign image signing; no GitHub Release; unsigned GHCR `v0.1.0` (`sha256:a28c4d2f17fc…`) kept public, replaced by v0.1.1 |
| Base mirror (PR #15, `ba652a28`, 2026-10-30 10:58 AM EDT) | Fedora bootc 45 pinned in `image/fedora-bootc.digest`, mirrored to `ghcr.io/awkward8787/fedora-bootc` | Merged |
| v0.1.1 (tag on `ba652a28`, Release published 2026-10-30 11:51 AM EDT) | First signed publish: qcow2/ISO/binaries for aarch64 and x86_64, signed `SHA256SUMS`, signed GHCR image, `:latest` moved | Published; [release page](https://github.com/Awkward8787/DARKJUANDER/releases/tag/v0.1.1) |
| Open follow-up PR #16 | Mirror pin, smoke wait and `:latest` ranking hardening | Open, CI green |
| Phase 0 remainder | Boot in UTM; load SELinux domain; bootc signature policy | Planned |
| Phase 1 MVP | Streaming, Anthropic API, llama.cpp as shared process, per-request cgroups, desktop + trusted approval dialog, onboarding Jaamal's agents, outside review | Planned |
| Phases 2-4 | Installable release, stronger isolation, ecosystem | Planned (PRD section 11) |

## 12. Assumptions and constraints

Built by Cursor cloud agents (owner bot: Lingxi's Engineer Bot) with separate security-review passes; solo-founder bandwidth; Apple silicon gives the guest no TPM or Secure Boot chain; building images needs a Linux host (podman, bootc-image-builder with privileged loop devices).

## 13. Risks

| Risk | Mitigation |
| --- | --- |
| AI with system access is the biggest risk | Least privilege, fail-closed approvals, audit, outside reviews before phase exits |
| Demos mistaken for enforcement | README "What the demos do not prove" section; status markers in these docs |
| Big vendors ship AI in their OSes | Compete on model choice, openness and containment |
| Local models need lots of RAM | PSI fallback to smaller model; cloud tier opt-in |
| Name/trademark conflict | Search before any public release (open question) |
| Approval fatigue | Scoped grants, batching (planned) |
| UMBRA gains may be small | Ship security features first; measure vs. plain llama-server |
| Unsigned v0.1.0 image still public on GHCR | Kept on purpose; `:latest` now points at signed v0.1.1; v0.1.1 release notes say v0.1.1 replaces it |
| Quay prunes the pinned Fedora base digest | Mirror in GHCR (PR #15); v0.1.0 source can no longer be rebuilt for this reason |
| Published default console password (`darkjuander`) on downloadable images | Expired on first boot by `dj-expire-password.service`; sshd off and inbound dropped; v0.1.1 QEMU smoke on both arches required and reported `password-expired: yes` |

## 14. Stakeholders and approvals

Owner and decision-maker: Jaamal (Anthony Jaamal Jones), Vibe Virtue Enterprise LLC, Valdosta, GA. Builder: Cursor cloud agents (Lingxi's Engineer Bot). Reviewers: security review passes recorded in `/workspace/djr-pr1-review/` and `/workspace/djr-pr4-review/`. PRs #1-#3 and #5-#8 were merged 2026-10-28/29; #4, #9 and #14 on 2026-10-29. Brand kit owner per `LICENSE-BRAND.md`: Anthony Jaamal Jones and Vibe Virtue Enterprise LLC (ownership split UNCONFIRMED).

## 15. Glossary

| Term | Meaning |
| --- | --- |
| DARKJUANDER | Working name of Jaamal's AI-security Linux OS (Vibe Virtue Enterprise LLC); trademark not searched |
| UMBRA | Working name of the userspace "neural core": model registry, trust-partitioned prefix cache and label/PSI-aware router (`services/umbra`, daemon `umbrad`) |
| dj-base | The hardened Fedora bootc 45 image defined by `image/Containerfile` |
| bootc | Image-based Linux: the OS is shipped as an OCI container image with read-only root and A/B updates |
| Label | Pair of integrity (`trusted`/`untrusted`) and confidentiality (`public`/`private`/`secret`) plus origin and owner; joins take the stricter value |
| Session floor | Highest confidentiality a session has touched; once raised, later calls in that session keep it |
| Span handle | Opaque id returned instead of private file bytes by a mediated read |
| PSI | Linux pressure stall information (`/proc/pressure/memory`), used as a router input |
| Principal | A caller identity in `umbrad.yaml` with bearer token, trust domain, `local_only` and grants |
| Grant | Capability string such as `model.use:default-chat`, `files.read:<path>`, `provider.use:xai` |
| Approve decision | Policy result that needs a human prompt; there is no trusted dialog yet, so it is logged and denied (fail-closed) |
| dj-auditd | Separate signer daemon (user `dj-audit`) that signs audit checkpoints with an ed25519 key |
| SIWC | Sign in with ChatGPT, OpenAI's public-client flow used by `dj provider login openai` |
| Umbra Eclipse | Recommended and default logo direction (direction A, `branding/BUILD_DIRECTION` = `eclipse`): dark disc over a violet crescent with a three-node route inside |
| GHCR | GitHub Container Registry; release images go to `ghcr.io/awkward8787/darkjuander` |
| cosign (keyless) | Sigstore signing tool; signs with a short-lived Fulcio certificate bound to the GitHub OIDC identity `…/release.yml@refs/tags/v…`, logged in Rekor |
| Dry run | `release.yml` run from `pull_request` or `workflow_dispatch` with `dry_run` true; builds artifacts but never publishes, version `0.0.0-dry.<12 hex>` |
| Smoke test | `scripts/smoke-qemu.sh`: boots the qcow2 under QEMU on the runner and requires multi-user target, `UMBRAD_SOCKET_OK`, the PAM password-expiry prompt, a completed password change and `sudo` accepting it |
| Staging tag | `ci-<run id>[-arch]` tags the publish job pushes before signing; internal, never to be `bootc switch`ed to |
| Base image mirror | `ghcr.io/awkward8787/fedora-bootc`, a GHCR copy of the Fedora bootc 45 index pinned in `image/fedora-bootc.digest`; written only by `scripts/mirror-fedora-bootc.sh --write` from `main` |
| Pin tag | Mirror tag `45-` plus the first 12 hex digits of the pinned digest (`45-479ad7d16893`); never moved to another digest |

## 16. Open questions

| Question | Owner | Blocks |
| --- | --- | --- |
| v0.1.0 stays public and unsigned at `ghcr.io/awkward8787/darkjuander@sha256:a28c4d2f17fc…` (tags `v0.1.0`, `v0.1.0-aarch64`, `v0.1.0-x86_64`); v0.1.1 replaces it and `:latest` now points at the signed v0.1.1 index. Keep v0.1.0 public indefinitely, or remove it later? (Its source can't be rebuilt: Quay pruned the old base digest.) | Jaamal | Package cleanup |
| `docs/INSTALL.md` at tag v0.1.1 still says "No newer tag has been published" and tells users not to switch to `:latest`. That text predates v0.1.1. Update it in a follow-up code-docs PR? | Jaamal / builder | Accurate install instructions |
| PR #16 ("Harden mirror pins, smoke waits, and latest ranking") is open with green CI. Merge before the next tag? | Jaamal (Lingxi merges) | Next release |
| The base mirror also has a tag `45` on the same digest as `45-479ad7d16893`. `scripts/mirror-fedora-bootc.sh` writes only the `45-<12 hex>` tag, so who created `45` is UNCONFIRMED. Keep or remove? | Builder | Mirror hygiene |
| Logo direction: the brand kit defaults to Umbra Eclipse; Label Tag, Lattice Gate and Route Monogram remain as alternatives. Confirm Eclipse as primary (or pick another and rerun `build.py --direction`), and whether to delete the unused directions. | Jaamal | Final brand, merch, social |
| Tagline: the README now leads with "Agents inside a boundary you can audit." (brand guide recommendation over four other options). Confirm. | Jaamal | Public copy |
| Brand licensing and ownership: `branding/LICENSE-BRAND.md` makes logos, marks, wallpapers, boot art and social cards proprietary ("All rights reserved") to Anthony Jaamal Jones and Vibe Virtue Enterprise LLC jointly, says DARKJUANDER is "used as a trademark of Vibe Virtue Enterprise LLC", and allows ™ only if the owner decides. Confirm the owner entity (person vs LLC), whether to use ™, and that forks must rebrand. | Jaamal | Brand-permission requests, any trademark filing |
| Trademark/domains: first-pass name check (2026-10-28, `branding/research/name-check.md`) found 0 USPTO records for DARKJUANDER and all seven checked domains appearing unregistered; UMBRA is crowded (Reg. No. 5182088 and others). Attorney search, buy darkjuander.com/.dev, claim handles? | Jaamal | Any public release |
| Published console password: downloadable images ship user `dj` with the published password `darkjuander` (`image/console-password.txt`, documented as not a secret); `dj-expire-password.service` expires it on first boot, and the v0.1.1 QEMU smoke reported `password-expired: yes`. Accept this model long-term, or require first-boot password setup with no default? | Jaamal | Future releases |
| Dependabot PRs #10-#13 bump pinned actions across major versions (checkout 4.4.0→7.0.1, download-artifact 4.3.0→8.0.2, upload-artifact 4.6.2→7.0.2, action-gh-release 2.6.2→3.0.3); CI and release dry runs are green. Merge, and re-review the release path after? | Jaamal / builder | Next release |
| Has the GitHub social preview (`branding/social/darkjuander-social-1280x640.png`) been uploaded in repo Settings? UNCONFIRMED (manual step). | Jaamal | Repo presentation |
| Licensing: the code is Apache-2.0 by a reversible Phase 0 default (`docs/decisions/0001`), not Jaamal's decision. Keep Apache-2.0, choose GPL-3.0, or go source-available/closed? The repo is public. | Jaamal | Public announcements, contributions |
| Desktop environment (GNOME/KDE/COSMIC/Sway) for the approval dialog and control center (PRD Q2). | Jaamal | UI-1 to UI-5, PB-4/PB-6 |
| Cloud providers at MVP: xAI and OpenAI adapters exist (off by default); is Anthropic required (GW-2)? | Jaamal | Phase 1 exit |
| Which of Jaamal's agent bots move first, and what grants do they need (PRD Q8)? | Jaamal | Phase 1 exit |
| Budget for an outside security review before any public beta (PRD Q10)? | Jaamal | Phase exit gates |
| Business model (free OS + support, Pro features, hardware bundles) is unset. | Jaamal | Pricing, PRD success metrics |

## 17. Changelog

| Date | Version | Change | Reason |
| --- | --- | --- | --- |
| 2026-10-29 | 1.0 | Initial reverse-documented pack from `main` @ `4176cf24` (PRs #1-#3, #5-#8 merged). | Repo moved from empty (awaiting code) to 7 merged PRs on 2026-10-28/29; found in the 2026-10-29 daily docs sync |
| 2026-10-30 | 1.1 | Summary, FR-001/002/014/016/017 status, new FR-018-FR-020, NFR-007/008, content, scope by release (v0.1.0 failure, v0.1.1 pending), risks, stakeholders, open questions | PRs #4 (`a410bf40`, 2026-10-29 12:58 PM EDT), #9 (`44bec1f1`, 1:47 PM EDT) and #14 (`9660261c`, 7:26 PM EDT) merged to `main` (now `9660261cbc`); v0.1.0 tag publish failed; found in the 2026-10-30 daily docs sync |
| 2026-10-30 | 1.2 | Status, summary, FR-001/002/014/017, NFR-007, scope by release (PR #15 mirror, v0.1.1 signed release, v0.1.0 kept), risks, open questions | Signed release v0.1.1 (https://github.com/Awkward8787/DARKJUANDER/releases/tag/v0.1.1) published 2026-10-30 11:51 AM EDT from tag on `ba652a28` after PR #15 merged at 10:58 AM EDT; docs PR approved by Jaamal 2:28 PM EDT |

## 18. Sources

- v1.1: cursor-github `compare_commits 4176cf24...9660261c` (3 commits, 166 files) with per-file patches; `get_branch main`; no clone or download
- v1.1: Files read from the patches: `branding/{README.md,BUILD_DIRECTION,LICENSE-BRAND.md,brand-guide.md,src/palette.json,grub/README.md,grub/darkjuander/theme.txt,plymouth/README.md,research/{name-check,competitors,contrast}.md}`, `branding/logo/{eclipse,tag,lattice,route}/svg/*`, `.github/workflows/{ci,release}.yml`, `.github/dependabot.yml`, `.github/ISSUE_TEMPLATE/brand-permission.yml`, `image/Containerfile`, `image/systemd/dj-expire-password.service`, `image/bootc/10-darkjuander.toml`, `image/console-password.txt` (header only; value not copied), `docs/{INSTALL,RELEASE_NOTES_TEMPLATE,BASE,ROADMAP,LICENSES,PRD}.md`, `docs/decisions/0001-phase0-defaults.md`, `README.md`
- v1.1: cursor-github `list_pull_requests` (all; #10-#13 Dependabot open), `list_tags` (only `v0.1.0` → `44bec1f1`), `list_releases` (empty), `list_workflow_runs` (release run 40924686961 on `v0.1.0`: image and iso jobs success, `publish release` failure), `list_workflow_run_jobs`, `get_job_logs` job 123643691424 (cosign `UNAUTHORIZED` at 2026-10-29 2:22 PM EDT)
- v1.1: Anonymous GHCR read 2026-10-30 8:20 AM EDT: `ghcr.io/v2/awkward8787/darkjuander/tags/list` → `v0.1.0-aarch64`, `v0.1.0-x86_64`, `v0.1.0`, `latest`; `v0.1.0` index digest sha256:a28c4d2f17fc3f02cdd59463641fbc875f240ba3e581de733016627613502009; no `.sig` tag
- v1.1: PR bodies #4, #9, #14; review reports `/workspace/djr-pr1-review/REVIEW-pr9*.md`, `REVIEW-pr14-{143d558,8d0c405,b84a598,792ef42}.md`, `/workspace/djr-pr4-review/REVIEW-{b1840e5,2529788}.md`
- v1.1: v1.0 sources: `main` @ `4176cf24` reconstruction, `/workspace/darkjuander/` planning folder, `/workspace/hivemind/master-context.md`
- v1.1: Generator: `/workspace/project-docs/_tools/gen/darkjuander_v1.1_2026-10-30.py`
- v1.2: cursor-github `list_releases` (v0.1.1, published 2026-10-30 11:51 AM EDT, 11 assets incl. `SHA256SUMS` and `SHA256SUMS.cosign.bundle`, release notes), `list_tags` (`v0.1.1` → `ba652a28`, `v0.1.0` → `44bec1f1`), `list_pull_requests` (#15 merged, #16 open, #10-#13 open), `list_pull_request_files` #15, `list_workflow_runs`, `list_workflow_run_jobs` run 41077487909 (all jobs success; mirror publish, cosign auth, cosign probe skipped; `publish did not finish` skipped), `list_branches`
- v1.2: files at tag v0.1.1 read via raw.githubusercontent.com: `scripts/smoke-qemu.sh`, `image/systemd/dj-expire-password.service`, `.github/workflows/release.yml`, `image/Containerfile`, `docs/INSTALL.md`, `docs/ROADMAP.md`, `README.md`; PR #15 and #16 bodies
- v1.2: own checks 2026-10-30 ~2:40 PM EDT with cosign v3 on the box: `cosign verify` of `ghcr.io/awkward8787/darkjuander:v0.1.1` with identity `…/release.yml@refs/tags/v0.1.1` passed (digest `sha256:02f054ac30b3…`, workflow SHA `ba652a28`); `cosign verify-blob` of the release `SHA256SUMS` with its bundle: Verified OK; `cosign verify` of `:v0.1.0` → no signatures found; anonymous GHCR reads: `latest` = `v0.1.1` = `sha256:02f054ac30b3…`, `v0.1.0` = `sha256:a28c4d2f17fc…`, `v0.1.1-aarch64` = `sha256:569d5334aade…`, `v0.1.1-x86_64` = `sha256:d9b69db53ca7…`; `ghcr.io/awkward8787/fedora-bootc` tags `45`, `45-479ad7d16893`, both `sha256:479ad7d16893…`
- v1.2 generator: `/workspace/project-docs/_tools/gen/darkjuander_v1.2_2026-10-30.py`
