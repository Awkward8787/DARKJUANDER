# Licenses

DARKJUANDER components in this repository are Apache-2.0. See `LICENSE`.

Third-party pieces we use but do not vendor:

| Piece | License | How it is used |
|---|---|---|
| Fedora bootc 45, Linux kernel, systemd, nftables, policycoreutils | Fedora's mix. The kernel is GPL-2.0. systemd is LGPL-2.1-or-later. nftables and policycoreutils are GPL-2.0. | Base image and programs we execute. Not linked into the Rust binaries. |
| bubblewrap | LGPL-2.0-or-later | Executed as `bwrap`. |
| llama.cpp v0.8.0 | MIT | Built from upstream by `scripts/build-llama.sh`. Not committed. |
| SmolLM2 135M Q4_K_M (ggml-org GGUF) | Apache-2.0 | Downloaded by `scripts/fetch-model.sh`. Not committed. |
| SELinux userspace (`libselinux`) | Public domain, as shipped by Fedora | Not linked. The draft module is text. |

Direct Rust dependencies, from `Cargo.toml`. Transitive crates are pinned in `Cargo.lock`.

| Crate | License |
|---|---|
| serde, serde_json, serde_yaml, thiserror, sha2, hex, clap, toml, tempfile | MIT OR Apache-2.0 |
| tokio, axum, hyper, hyper-util, tower, http, http-body-util, bytes, tracing, tracing-subscriber | MIT |
| landlock, libc, ureq | MIT OR Apache-2.0 |

No AGPL dependency is selected. No model weights, disk images, or llama.cpp sources are committed.
