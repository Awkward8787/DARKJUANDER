#!/usr/bin/env python3
"""Compute WCAG 2.x contrast ratios for the DARKJUANDER palette. Writes research/contrast.md."""
import json, pathlib
ROOT = pathlib.Path(__file__).resolve().parent.parent
P = json.loads((ROOT / "src/palette.json").read_text())

def lum(h):
    h = h.lstrip("#"); out = []
    for i in (0, 2, 4):
        c = int(h[i:i+2], 16) / 255
        out.append(c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4)
    r, g, b = out
    return 0.2126 * r + 0.7152 * g + 0.0722 * b

def ratio(a, b):
    la, lb = sorted((lum(a), lum(b)), reverse=True)
    return (la + 0.05) / (lb + 0.05)

def grade(r):
    return "AAA" if r >= 7 else "AA" if r >= 4.5 else "AA large / UI" if r >= 3 else "fail"

n, b = P["neutral"], P["brand"]
pairs = [
    ("Body text: lumen_100 on umbra_950", n["lumen_100"], n["umbra_950"]),
    ("Body text: lumen_100 on umbra_900", n["lumen_100"], n["umbra_900"]),
    ("Muted text: ash_400 on umbra_950", n["ash_400"], n["umbra_950"]),
    ("Muted text: ash_400 on umbra_800", n["ash_400"], n["umbra_800"]),
    ("Accent: violet on umbra_950", b["violet"], n["umbra_950"]),
    ("Accent: violet on umbra_800", b["violet"], n["umbra_800"]),
    ("Button label: umbra_950 on violet", n["umbra_950"], b["violet"]),
    ("Light mode text: umbra_950 on lumen_50", n["umbra_950"], n["lumen_50"]),
    ("Light mode text: umbra_950 on white", n["umbra_950"], "#FFFFFF"),
    ("Light mode accent: violet_deep on white", b["violet_deep"], "#FFFFFF"),
    ("Light mode accent: violet_deep on lumen_50", b["violet_deep"], n["lumen_50"]),
    ("Button label: white on violet_deep", "#FFFFFF", b["violet_deep"]),
    ("Divider: umbra_600 on umbra_950 (decorative only)", n["umbra_600"], n["umbra_950"]),
    ("Input border: umbra_500 on umbra_950 (UI part)", n["umbra_500"], n["umbra_950"]),
]
for k, v in P["semantic_dark"].items():
    pairs.append((f"Dark semantic {k} on umbra_950", v, n["umbra_950"]))
    pairs.append((f"Dark semantic {k} on umbra_800", v, n["umbra_800"]))
for k, v in P["semantic_light"].items():
    pairs.append((f"Light semantic {k} on white", v, "#FFFFFF"))
t = P["terminal"]
names = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"]
for i, c in enumerate(t["ansi"]):
    nm = ("bright " if i > 7 else "") + names[i % 8]
    pairs.append((f"Terminal {i:2d} {nm} on background", c, t["background"]))
pairs.append(("Terminal foreground on background", t["foreground"], t["background"]))
pairs.append(("Terminal selection_fg on selection_bg", t["selection_fg"], t["selection_bg"]))

lines = ["# DARKJUANDER contrast ratios", "",
         "Computed by `scripts/contrast.py` with the WCAG 2.x relative-luminance formula. "
         "4.5:1 is the AA minimum for body text, 3:1 for large text and UI parts, 7:1 is AAA.", "",
         "| Pair | Foreground | Background | Ratio | Grade |", "|---|---|---|---|---|"]
for name, fg, bg in pairs:
    r = ratio(fg, bg)
    lines.append(f"| {name} | `{fg}` | `{bg}` | {r:.2f}:1 | {grade(r)} |")
(ROOT / "research").mkdir(exist_ok=True)
(ROOT / "research/contrast.md").write_text("\n".join(lines) + "\n")
print("\n".join(lines))
