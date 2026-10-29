# DARKJUANDER brand guide

Version 1.0, October 2026. Owner: Anthony Jaamal Jones, Vibe Virtue Enterprise LLC.

This guide covers the name, the mark, the colors, the type, the voice, and how all of it shows up inside the OS. The source files live next to it in this folder. Run `python3 scripts/build.py` to regenerate every asset from `src/palette.json` and the fonts in `src/fonts/`.

DARKJUANDER is still a working name. A first search found no conflicting software marks (see `research/name-check.md`), but that search is not legal clearance. Do not use ® anywhere. See `LICENSE-BRAND.md`.

---

## 1. Positioning

**Positioning line**

> DARKJUANDER is a Linux workstation for people who run AI agents every day and want each one inside a boundary they can inspect, limit, and audit.

**Who it is for:** terminal-comfortable developers, solo founders, and security people who already run coding agents and local models.

**What it replaces:** hand-built containers, scattered logs, and hoping the agent with shell access behaves.

**How it is different:** the other security distros ship tools or compartments (see `research/competitors.md`). DARKJUANDER ships a boundary for AI agents: labels on every model call, a sandbox per agent, and one audit log for all of it.

### Three proof points

Each one points at code or tests in the repository. Anything not built yet is marked **planned**.

1. **Private data stays on the machine.** Every model call through UMBRA carries a data label. In demo scene 5, an agent resubmits bytes from a private file with a public label. The session stays private, and the hard rule keeps the call on the local model. The cloud engine's call count stays at 0. (`cargo test`, scene 5, mock engine.)
2. **One audit log you can check.** Model calls, tool calls, and broker decisions go into a hash-chained log with a signed checkpoint. `dj audit verify` catches truncation and swapped fields. (Shipped in the userspace. Log encryption is **planned**.)
3. **Agents do not share what they should not.** A second agent with the same prefix gets a miss and no trace of the first agent's domain in its response (scene 2, mock engine; timing not measured). `dj sandbox run` starts each agent under bubblewrap with Landlock and a seccomp filter, no network, and no capabilities. (Shipped. The seccomp profile is a denylist today; an allowlist is **planned**.)

**Planned, not shipped. Never describe these as available:** a bootable disk image, signed images and A/B updates, a desktop shell and trusted approval dialog, real cloud provider clients (Grok, ChatGPT), and the Section 7.9 performance targets.

---

## 2. Who we are talking to

| Buyer | What they run today | What they want | What wins them |
|---|---|---|---|
| **The agent operator** (solo founder, small team lead) | Several coding and research agents, each with API keys, across local and cloud models | Separate keys, files, and limits per agent, one place to see what each did, cost control | A single audit trail and per-agent policy without writing their own container setup |
| **The careful developer** | Claude Code, Codex, Aider, or similar with shell access on their main desktop | A local model API their tools already speak, and a sandbox for agents that run commands | The OpenAI-shaped `umbrad` socket and `dj sandbox` |
| **The privacy-minded Linux user** | Fedora, Arch, or Debian, avoids AI features in mainstream OSes | No telemetry, local models by default, a clear view of what leaves the machine | No phone-home by default (PRD BASE-8) and labels that keep private data local |
| **The security researcher / red-teamer** (later) | Kali or Parrot VMs, custom harnesses | A platform to test agents and models under policy, with logs that hold up | Policy engine plus verifiable audit log |

Not v1 targets: mainstream consumers, enterprise fleets, gamers who need anti-cheat. Early users will likely try it in a VM first. Copy should make that easy and say so.

---

## 3. Name and taglines

**Name:** DARKJUANDER. Always all caps in text and in the wordmark. One word. No space, no hyphen, no "Dark Juander."

**Sub-brand:** UMBRA is the neural core (Unified Model, Buffer and Routing Architecture). Other companies already use UMBRA in software and networking, including a registered UMBRA mark for 3D software and UMBRA Technologies in secure networking. Treat UMBRA as a component name, write it as "the UMBRA core" or "DARKJUANDER UMBRA," and do not build a separate public brand on it until an attorney clears it.

### Five tagline options

1. **Run your agents in the dark.** Short and owns the name. Risk: "dark" can read as shady to a buyer who has not seen the product.
2. **Agents inside a boundary you can audit.** Says exactly what the product does. Longer.
3. **Every model call, labeled and logged.** Concrete, technical, provable from the demos.
4. **Your machine. Your models. Your rules.** Familiar shape, close to other privacy products.
5. **Local by default. Logged by design.** Two facts, both true today in the userspace.

**Recommended:** **Agents inside a boundary you can audit.** It is the only one that names the buyer's real worry (agents with shell access) and the fix in one line, and every word maps to something in the repo. Use "Every model call, labeled and logged." as the technical secondary line in READMEs and talks.

---

## 4. Voice

DARKJUANDER talks like a senior engineer explaining their own system to a peer: specific, calm, and clear about limits.

### Trait 1: Exact

We name the mechanism, the command, the file.

| We say | We don't say |
|---|---|
| "`dj audit verify` checks the hash chain against the signed checkpoint." | "Military-grade tamper protection." |
| "The sandbox drops the network and all capabilities." | "Bulletproof isolation." |

### Trait 2: Honest about limits

We say what is planned, what is a target, and what the demo does not prove. This is the brand's sharpest edge against tool-catalog distros.

| We say | We don't say |
|---|---|
| "No disk image has been built yet. Here is the build path." | "Download DARKJUANDER today." (until it is true) |
| "The seccomp profile is a denylist for now." | "Fully hardened." |

### Trait 3: Quietly confident

Dark palette, short sentences, no hype. We let the boundary do the talking.

| We say | We don't say |
|---|---|
| "Agents get a boundary. You get the log." | "Unleash the power of AI like never before!" |
| "Built on Fedora bootc." | "Revolutionary new operating system." |

**Words we avoid:** hacker, elite, unhackable, military-grade, bulletproof, revolutionary, game-changing, next-gen, unleash, seamless, leverage. No skulls, hoods, or green code rain in words either.

**Mechanics:** sentence case for headings, numerals for numbers, commands in code formatting, no exclamation marks in product copy, no em dashes.

---

## 5. Sample copy

### Homepage hero

> **Agents inside a boundary you can audit.**
>
> DARKJUANDER is a Linux workstation built on Fedora bootc for people who run AI agents every day. Each agent gets its own sandbox. Every model call carries a label that decides where it may run. Everything lands in one log you can verify.
>
> The userspace and demos run today. The bootable image is next.
>
> [Read the code] [See what works today]

### GitHub README hero

> # DARKJUANDER
>
> **Agents inside a boundary you can audit.**
>
> A Linux system for running AI agents with a boundary around them. Fedora bootc plus a userspace neural core called UMBRA. Every model call is labeled, every agent is sandboxed, and every decision goes into a hash-chained audit log.
>
> Status: userspace and five demo scenes run on a mock engine (`cargo test`, `dj demo`). No disk image has been built or booted yet. See "What the demos do not prove" before treating a pass as a security boundary.

### Boot and login greeting

Plymouth (under the logo, ash_400, Inter 14):
> Starting DARKJUANDER

LUKS prompt:
> Disk passphrase

Login screen:
> Welcome back. Your agents are where you left them.

(Planned. That sentence says the agents are already there. It is not installed. The image writes a neutral `/usr/lib/issue.d/50-darkjuander.issue` line instead.)

First terminal after login (motd):
> DARKJUANDER  |  umbrad: running  |  audit: verified

(Planned. The motd values must come from real status calls, never hard-coded. Today the agent view is the `GET /v1/dj/top` admin route; a `dj top` command does not exist yet.)

### Release announcement opener

> DARKJUANDER 0.2 is out. This release adds per-agent prefix cache isolation and moves PSI-based routing into the default config. It does not include a bootable image yet; that is still the next milestone. Here is what changed, what we tested, and what we have not.

(Version number and contents are examples. Fill in from the real changelog.)

### X / Reddit post

Template for Jaamal to fill in. This is not his post, and it does not speak for him.

X:
> I'm building DARKJUANDER, a Linux workstation for running AI agents inside a boundary you can audit.
>
> [What you run today, in your own words.]
>
> Each agent gets a sandbox. Every model call gets a label. Private data is kept off cloud models by a hard rule (tested on a mock engine), and the calls land in one verifiable log.
>
> The userspace and demos run today. No disk image has been booted yet.
> github.com/Awkward8787/DARKJUANDER

Reddit (r/linux, r/selfhosted, r/LocalLLaMA):
> **Title:** [One factual sentence about the sandbox and the log.]
>
> [Your first paragraph, in your own words. Do not say private data cannot leave the machine. The tested behavior is a hard rule that keeps a labeled call on the local model in the mock-engine demo. Say that the userspace runs and that no disk image has been booted.]

---

## 6. Color

Source of truth: `src/palette.json`. Ratios computed by `scripts/contrast.py` with the WCAG 2.x formula; full table in `research/contrast.md`.

### Brand

| Name | Hex | Use |
|---|---|---|
| Umbra | `#0A0C11` | Primary background. The shadow. |
| Violet | `#9D8CFF` | The light at the edge. Accent on dark: route end node, focus rings, links, progress. |
| Violet Deep | `#5B48D6` | Accent on light backgrounds and primary buttons on light. |

### Neutrals

| Name | Hex | Use |
|---|---|---|
| umbra_950 | `#0A0C11` | App and desktop background |
| umbra_900 | `#11141B` | Panels, cards |
| umbra_800 | `#1B2029` | Raised surfaces, hover |
| umbra_700 | `#2A2F3D` | Selection, pressed |
| umbra_600 | `#3A4252` | Dividers (decorative only) |
| umbra_500 | `#5A6377` | Input borders, icons at rest |
| ash_400 | `#8A93A6` | Secondary text on dark |
| ash_200 | `#C3C9D6` | Light-mode borders, disabled text on dark |
| lumen_100 | `#E6E9F0` | Primary text on dark |
| lumen_50 | `#F4F5F8` | Light-mode background |

### Status

| Role | On dark | On light |
|---|---|---|
| OK / allowed | `#3DD68C` | `#0E7A47` |
| Warn / approval needed | `#F5B841` | `#8A5A00` |
| Danger / denied | `#FF5C5C` | `#C22B2B` |
| Info | `#4CB8FF` | `#0B6BB3` |

Violet is never a status color. That keeps "this is the brand" separate from "this is allowed or denied."

### Contrast (WCAG 2.x)

| Pair | Ratio | Grade |
|---|---|---|
| lumen_100 on umbra_950 (body text) | 16.09:1 | AAA |
| ash_400 on umbra_950 (secondary text) | 6.34:1 | AA |
| ash_400 on umbra_800 | 5.29:1 | AA |
| violet on umbra_950 (links, accent) | 7.11:1 | AAA |
| violet on umbra_800 | 5.94:1 | AA |
| umbra_950 on violet (button label) | 7.11:1 | AAA |
| umbra_950 on lumen_50 (light mode text) | 17.94:1 | AAA |
| violet_deep on white (light accent) | 6.28:1 | AA |
| white on violet_deep (light button) | 6.28:1 | AA |
| umbra_500 on umbra_950 (input border) | 3.25:1 | passes 3:1 for UI parts |
| umbra_600 on umbra_950 (divider) | 1.94:1 | decorative only, never for meaning |
| Status on umbra_950: ok / warn / danger / info | 10.43 / 11.01 / 6.46 / 8.95 | AA or better |
| Status on white: ok / warn / danger / info | 5.39 / 5.93 / 5.72 / 5.57 | AA |

Rules: body text must be lumen_100 or ash_400 on dark, umbra_950 on light. Never put violet text on violet_deep or on lumen_50 at small sizes (use violet_deep on light).

---

## 7. Type

All three families are free under the SIL Open Font License 1.1. License files are in `src/fonts/OFL-*.txt`. The OFL lets us bundle, embed, and ship them in the OS image; we must keep the license with the font files and not sell the fonts on their own.

| Role | Family | Weights | Where |
|---|---|---|---|
| Display | **Space Grotesk** | 500 (wordmark, headlines), 700 (heavy headlines) | Wordmark, hero lines, slide titles |
| UI | **Inter** | 400, 500, 600 | Desktop shell, dashboards, docs body, login |
| Mono | **JetBrains Mono** | 400, 700 | Terminal default, code, log viewers, the mono line on cards |

Space Grotesk is the display face because the outlined wordmark keeps its spurred D and barred J. Inter is the UI face because lumen_100 on umbra_950 stays readable at 14 px (see the contrast table). JetBrains Mono is the terminal and audit face so code and hashes do not look like the wordmark.

Wordmark: Space Grotesk Medium, all caps, tracking +150 (0.15 em). In the logo files the letters are outlined to paths, so the wordmark renders the same with or without the font installed. Do not retype the wordmark in a live font; use the SVG.

Type scale for UI (Inter): 12 / 14 / 16 / 20 / 24 / 32. Headlines (Space Grotesk): 40 / 54 / 72.

---

## 8. Logo

### Recommended direction: Umbra Eclipse

A dark disc (the umbra) covers a violet light, leaving a crescent at the upper right. Inside the disc, a three-node route runs from a hollow start node to a solid violet end node: a model call moving through UMBRA, labeled and tracked.

Why this one:
- "Umbra" is literally the dark center of an eclipse shadow, so the mark ties straight to the core's name.
- It avoids the shield and padlock shapes that crowd the security category.
- Violet is clear of Kali's blue, Parrot's cyan, and BlackArch's red, and it never collides with status colors.
- A simplified small variant (disc, crescent, end node only) stays readable at 16 px.

The other three directions (Label Tag, Lattice Gate, Route Monogram) are in `logo/` as SVG masters in case the owner prefers one. `python3 scripts/build.py` writes `logo/logo-directions.png`, which compares all four. That PNG is not committed.

### Files

```
logo/eclipse/svg/eclipse-icon-{dark,light,mono-black,mono-white}.svg
logo/eclipse/svg/eclipse-icon-small-*.svg       (use at 32 px and below)
logo/eclipse/svg/eclipse-horizontal-*.svg
logo/eclipse/svg/eclipse-stacked-*.svg
logo/eclipse/png/icon/eclipse-icon-{16..1024}-{transparent,on-dark,light-transparent}.png
logo/eclipse/png/{horizontal,stacked}/...
```

"dark" means the version for dark backgrounds. "light" is for light backgrounds. Use mono files for one-color print, etching, and embroidery.

### Clear space

Keep empty space around the logo equal to **half the icon's diameter** (0.5x) on all sides. For the wordmark alone, use the cap height of the letter D. Nothing (text, edges, other logos) goes inside that space.

### Minimum size

| Asset | Digital minimum | Print minimum |
|---|---|---|
| Icon (full) | 40 px | 10 mm |
| Icon (small variant) | 16 px | 5 mm |
| Horizontal lockup | 120 px wide | 30 mm wide |
| Stacked lockup | 80 px wide | 20 mm wide |

Below 40 px, switch to the `icon-small` files.

### Do

- Use the supplied SVGs or PNGs.
- Put the dark version on umbra_950, umbra_900, or dark photos with enough contrast.
- Put the light version on lumen_50 or white.
- Use the mono version when only one color is available.

### Don't

- Don't redraw, retype, or re-track the wordmark.
- Don't change the colors, add gradients, glows, or drop shadows to the mark.
- Don't rotate, stretch, or outline it.
- Don't move the crescent to another side or change the route.
- Don't place the dark version on mid-gray or busy backgrounds.
- Don't add skulls, hoods, masks, padlocks, or code rain around it.
- Don't combine it with another product's logo into a new lockup without approval.

---

## 9. Imagery

**Look:** near-black fields, one violet light source per image, and a 1 px grid. The grid is the trust-domain lattice, the same spacing a diagram uses when it shows agents in separate domains. It is not a texture. Light comes from the edge, the way it does around an eclipse. The eclipse mark itself has no glow. Diagrams and real screenshots beat illustration.

**Use:** real terminal output and real UI from the repo, architecture diagrams redrawn in the palette, close crops of hardware with low-key lighting, abstract eclipses and routes made from the brand shapes.

**Avoid:** hooded hackers, skulls, Guy Fawkes masks, green code rain, glowing padlocks, stock "cyber" globes, glowing brains, and fake dashboards with invented numbers.

**Screenshots:** show real commands and real output. If a screen is a mockup of a planned feature, label it "Concept, not shipped" in the image.

---

## 10. In the OS

| Surface | What it uses | File |
|---|---|---|
| **GRUB** | Artwork only. bootupd's static config does not load a theme, so the image does not install one. | `grub/darkjuander/`. See `grub/README.md` |
| **Plymouth** | Centered icon, wordmark scaled with the icon, 240x4 violet progress bar, passphrase label from Plymouth's prompt text in Inter 14 | `plymouth/darkjuander/`. The image build runs dracut and CI checks the initramfs with `lsinitrd`. Not booted |
| **Login (GDM or greeter)** | Planned. No greeter is installed. The "Welcome back" line is not installed | wallpapers, palette |
| **Wallpaper** | Large eclipse at right, route faint inside the disc, small wordmark at lower left | `wallpapers/darkjuander-{3840x2160,2560x1600,1920x1080}.{svg,png}` |
| **Terminal** | Background umbra_950, text lumen_100, violet cursor, JetBrains Mono 12 to 14 | `terminal/` for Xresources, foot, kitty, Alacritty, GNOME Terminal |
| **Dashboard UI (planned `dj-shell`; today the `/v1/dj/top` admin view)** | Inter UI, umbra_900 cards on umbra_950, status colors only for allowed/approval/denied, violet only for focus and brand, audit hashes in JetBrains Mono ash_400 | palette, type |
| **App and OS icon** | Icon dark variant on an umbra_950 rounded tile; small variant at 32 px and below | `os-icon/hicolor/`, `web/app-icon.svg` |
| **Web** | Favicon (small mark on tile), Apple touch icon, OG card 1200x630 | `web/` |

**Dashboard UI rules:**
- Agent rows show name, trust domain, model, label floor, and last decision.
- Denied actions use danger red text plus an icon, never color alone.
- Approval prompts must be visually distinct from anything an agent can draw (planned: trusted dialog in `dj-shell`).
- Numbers shown in the UI come from live calls. Never ship a screenshot with made-up counts.
