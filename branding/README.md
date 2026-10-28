# DARKJUANDER branding

Brand identity kit for DARKJUANDER. Start with [brand-guide.md](brand-guide.md). Usage terms are in [LICENSE-BRAND.md](LICENSE-BRAND.md); these files are not under the code's Apache-2.0 license.

Recommended logo, and the default everywhere in this repository: **Umbra Eclipse** (`logo/eclipse/`, direction A). `BUILD_DIRECTION` is `eclipse`. Comparison of all four directions: [logo/logo-directions.png](logo/logo-directions.png).

## Folder map

| Path | What is in it |
|---|---|
| `brand-guide.md` | Positioning, buyers, taglines, voice, sample copy, color, type, logo rules, OS application |
| `LICENSE-BRAND.md` | Brand asset terms |
| `concepts/` | Four AI-generated concept sketches used to explore directions. Reference only, not final art |
| `logo/<direction>/svg/` | SVG masters for all four directions: icon, icon-small, horizontal, stacked × dark, light, mono-black, mono-white. Text is outlined |
| `logo/eclipse/png/` | Full PNG set for the default direction: icons 16 to 1024 px, lockups at 512/1024/2048 px wide |
| `logo/{tag,lattice,route}/png/` | One preview each (`*-horizontal-dark-512w.png`). `scripts/build.py` regenerates the rest |
| `logo/logo-directions.png` | Side-by-side of the four directions on dark and light |
| `logo/palette-swatches.png` | Palette sheet |
| `wallpapers/` | 3840x2160, 2560x1600, 1920x1080 (SVG + PNG) |
| `plymouth/darkjuander/` | Boot splash theme (script module) |
| `grub/darkjuander/` | GRUB theme skeleton (`.pf2` fonts must be built, see `grub/README.md`) |
| `terminal/` | Xresources, foot, kitty, Alacritty, GNOME Terminal |
| `os-icon/hicolor/` | App icon in freedesktop icon-theme sizes |
| `web/` | Favicon (SVG + ICO), app icons, Apple touch icon, OpenGraph card 1200x630 |
| `research/` | Competitor scan, name check, contrast table |
| `src/` | `palette.json` (source of truth) and OFL fonts |
| `scripts/` | `build.py` (regenerates everything), `contrast.py` |

## Rebuild

Needs Python 3, `fonttools`, `Pillow`, and `rsvg-convert` (librsvg). `grub2-mkfont` is optional.

```
pip install fonttools pillow
python3 scripts/build.py                       # all logo directions, OS assets in Umbra Eclipse
python3 scripts/build.py --direction lattice   # OS assets in another direction (eclipse, tag, lattice, route)
python3 scripts/build.py --no-png              # SVG only
python3 scripts/contrast.py                    # rewrites research/contrast.md
```

Change a color in `src/palette.json`, rerun both scripts, and every asset and the contrast table update.

## In this repository

This folder is `branding/` at the root of `Awkward8787/DARKJUANDER`. The image definition installs the Plymouth theme, the GRUB theme (`.pf2` fonts are built in the image, not committed), the wallpapers, the hicolor icon, and the terminal schemes. That install has not been booted. See [../docs/BASE.md](../docs/BASE.md).
