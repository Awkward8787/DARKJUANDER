# Licenses

Code in this repository is Apache-2.0, including `branding/scripts/`, `branding/src/palette.json`, and `branding/terminal/`. See `LICENSE`. Logos, marks, wallpapers, Plymouth and GRUB artwork, and social cards are proprietary to Anthony Jaamal Jones and Vibe Virtue Enterprise LLC. See `branding/LICENSE-BRAND.md`. DARKJUANDER is not a registered trademark. The Apache-2.0 grant does not cover those names or marks.

Vendored with the brand kit, and not under Apache-2.0:

| Piece | License | How it is used |
|---|---|---|
| Space Grotesk, Inter, JetBrains Mono (`branding/src/fonts/`) | SIL Open Font License 1.1 (`OFL-*.txt` next to each font). Inter's copyright line matches the font name table, 2016. | Shipped in the image under `/usr/share/fonts/darkjuander/`. The license files ship with them. |
| Logos, wallpapers, Plymouth artwork, GRUB artwork, icons, social cards | Proprietary. `branding/LICENSE-BRAND.md` | Plymouth, wallpapers, and icons are installed by the image definition. GRUB artwork is not installed. |

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
