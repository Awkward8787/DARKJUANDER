# Terminal color scheme

Generated from `src/palette.json` by `scripts/build.py`. Every ANSI color except color 0 passes 4.5:1 on the background; color 0 is meant as a background color (see `research/contrast.md`).

| File | Use |
|---|---|
| `darkjuander.Xresources` | `xrdb -merge darkjuander.Xresources` (xterm, urxvt, st patches) |
| `foot.ini` | Paste the `[colors]` and `[cursor]` sections into `~/.config/foot/foot.ini` |
| `kitty.conf` | Copy to `~/.config/kitty/darkjuander.conf`, then add `include darkjuander.conf` to `kitty.conf` |
| `alacritty.toml` | Copy to `~/.config/alacritty/darkjuander.toml`, then add it to `general.import` (older Alacritty: top-level `import`) |
| `gnome-terminal.dconf` | `dconf load /org/gnome/terminal/legacy/profiles:/:d4c1a5e0-7d1f-4c2b-9a3e-6f0b2c8e1d55/ < gnome-terminal.dconf`, then add `d4c1a5e0-7d1f-4c2b-9a3e-6f0b2c8e1d55` to the profiles `list` key |
| `palette.txt` | Plain hex list for anything else |
| `preview.png` | What it looks like |

Recommended font: JetBrains Mono 12 to 14.
