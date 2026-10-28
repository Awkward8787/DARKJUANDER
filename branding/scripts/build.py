#!/usr/bin/env python3
"""
DARKJUANDER brand build.

Generates every SVG master from code, then exports PNGs with rsvg-convert.
Text is converted to outlines from the OFL fonts in src/fonts, so no SVG
depends on installed fonts.

  python3 scripts/build.py                       # logos for all directions + OS assets for the default
  python3 scripts/build.py --direction lattice   # rebuild OS assets around another direction
  python3 scripts/build.py --no-png              # SVG only

Directions: eclipse (recommended), tag, lattice, route.
Needs: python3, fontTools, Pillow, rsvg-convert. Optional: grub-mkfont / grub2-mkfont.
"""
import argparse, json, math, pathlib, shutil, subprocess, sys
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.pens.boundsPen import BoundsPen

ROOT = pathlib.Path(__file__).resolve().parent.parent
PAL = json.loads((ROOT / "src/palette.json").read_text())
N, B = PAL["neutral"], PAL["brand"]
CACHE = ROOT / "src/fonts/.instances"

# ---------------------------------------------------------------- fonts
class Font:
    def __init__(self, file, **axes):
        CACHE.mkdir(parents=True, exist_ok=True)
        tag = "-".join(f"{k}{v}" for k, v in sorted(axes.items()))
        out = CACHE / f"{pathlib.Path(file).stem}-{tag}.ttf"
        if not out.exists():
            instancer.instantiateVariableFont(TTFont(ROOT / "src/fonts" / file), axes).save(out)
        self.f = TTFont(out)
        self.gs = self.f.getGlyphSet()
        self.cmap = self.f.getBestCmap()
        self.upm = self.f["head"].unitsPerEm
        self.cap = self.f["OS/2"].sCapHeight
        self.hmtx = self.f["hmtx"]

    def glyph(self, ch):
        return self.cmap[ord(ch)]

    def text(self, s, size, x=0.0, y=0.0, tracking=0.0, custom=None):
        """Return (svg path d, advance width). y is the baseline. tracking is in em/1000."""
        sc = size / self.upm
        pen = SVGPathPen(self.gs)
        cx = x
        for i, ch in enumerate(s):
            if ch == " ":
                cx += self.hmtx["space"][0] * sc + tracking * size / 1000
                continue
            g = self.glyph(ch)
            adv = self.hmtx[g][0]
            if custom and ch in custom:
                custom[ch](pen, cx, y, sc, adv)
            else:
                self.gs[g].draw(TransformPen(pen, (sc, 0, 0, -sc, cx, y)))
            cx += adv * sc
            if i < len(s) - 1:
                cx += tracking * size / 1000
        return pen.getCommands(), cx - x

    def measure(self, s, size, tracking=0.0, custom=None):
        return self.text(s, size, 0, 0, tracking, custom)[1]

    def stem(self):
        bp = BoundsPen(self.gs)
        self.gs[self.glyph("I")].draw(bp)
        return bp.bounds[2] - bp.bounds[0]

SG500 = Font("SpaceGrotesk.ttf", wght=500)
SG700 = Font("SpaceGrotesk.ttf", wght=700)
INTER = Font("Inter.ttf", wght=400, opsz=14)
INTER600 = Font("Inter.ttf", wght=600, opsz=14)
MONO = Font("JetBrainsMono.ttf", wght=400)

def lambda_A(font, tag_color_hook):
    """Custom crossbar-less A (a chevron) plus a square 'label tag' in the counter."""
    t = font.stem()
    lsb = font.hmtx[font.glyph("A")][1]
    def draw(pen, x, y, sc, adv):
        w = adv - 2 * lsb
        h = font.cap
        tp = t * 1.12
        yi = h * (1 - 2 * tp / w)
        pts = [(0, 0), (w / 2 - tp * 0.18, h), (w / 2 + tp * 0.18, h), (w, 0), (w - tp, 0), (w / 2, yi), (tp, 0)]
        pts = [(x + (lsb + px) * sc, y - py * sc) for px, py in pts]
        pen.moveTo(pts[0]); [pen.lineTo(p) for p in pts[1:]]; pen.closePath()
        s = t * 0.92
        tag_color_hook.append((x + (lsb + w / 2 - s / 2) * sc, y - s * sc, s * sc))
    return draw

# ---------------------------------------------------------------- color variants
VARIANTS = {
    "dark":       dict(fg=N["lumen_100"], accent=B["violet"],      disc=N["umbra_800"], inner=N["lumen_100"], line=N["lumen_100"], mono=False),
    "light":      dict(fg=N["umbra_950"], accent=B["violet_deep"], disc=N["umbra_950"], inner=N["lumen_50"],  line=N["umbra_950"], mono=False),
    "mono-black": dict(fg="#000000", accent="#000000", disc="#000000", inner="#000000", line="#000000", mono=True),
    "mono-white": dict(fg="#FFFFFF", accent="#FFFFFF", disc="#FFFFFF", inner="#FFFFFF", line="#FFFFFF", mono=True),
}

_uid = [0]
def uid(p):
    _uid[0] += 1
    return f"{p}{_uid[0]}"

# ---------------------------------------------------------------- icons (512 x 512 design box)
def icon_eclipse(c, small=False):
    """Umbra Eclipse: a shadow disc covering a violet light disc; a 3-node route inside the umbra."""
    dx, dy = (38, -28) if not small else (64, -46)
    R, gap = (178, 12) if not small else (168, 18)
    ox, oy = (256 - dx / 2, 256 - dy / 2)
    lx, ly = ox + dx, oy + dy
    m = uid("ecl")
    out = [f'<mask id="{m}c" maskUnits="userSpaceOnUse" x="0" y="0" width="512" height="512">'
           f'<rect width="512" height="512" fill="#fff"/><circle cx="{ox}" cy="{oy}" r="{R + gap}" fill="#000"/></mask>',
           f'<circle cx="{lx}" cy="{ly}" r="{R}" fill="{c["accent"]}" mask="url(#{m}c)"/>']
    if small:
        dot = (ox - 8, oy + 8, 46)
        if c["mono"]:
            out.insert(1, f'<mask id="{m}d" maskUnits="userSpaceOnUse" x="0" y="0" width="512" height="512"><rect width="512" height="512" fill="#fff"/>'
                          f'<circle cx="{dot[0]}" cy="{dot[1]}" r="{dot[2]}" fill="#000"/></mask>')
            out.append(f'<circle cx="{ox}" cy="{oy}" r="{R}" fill="{c["disc"]}" mask="url(#{m}d)"/>')
        else:
            out.append(f'<circle cx="{ox}" cy="{oy}" r="{R}" fill="{c["disc"]}"/>')
            out.append(f'<circle cx="{dot[0]}" cy="{dot[1]}" r="{dot[2]}" fill="{c["inner"]}"/>')
        return "\n".join(out)
    n1, n2, n3 = (ox - 92, oy + 62), (ox - 6, oy + 18), (ox + 70, oy - 66)
    sw = 14
    route = (f'<polyline points="{n1[0]},{n1[1]} {n2[0]},{n2[1]} {n3[0]},{n3[1]}" fill="none" '
             f'stroke-width="{sw}" stroke-linecap="round" stroke-linejoin="round"')
    if c["mono"]:
        out.insert(1, f'<mask id="{m}d" maskUnits="userSpaceOnUse" x="0" y="0" width="512" height="512"><rect width="512" height="512" fill="#fff"/>'
                      f'{route} stroke="#000"/>'
                      f'<circle cx="{n1[0]}" cy="{n1[1]}" r="24" fill="#000"/>'
                      f'<circle cx="{n2[0]}" cy="{n2[1]}" r="20" fill="#000"/>'
                      f'<circle cx="{n3[0]}" cy="{n3[1]}" r="30" fill="#000"/></mask>')
        out.append(f'<circle cx="{ox}" cy="{oy}" r="{R}" fill="{c["disc"]}" mask="url(#{m}d)"/>')
        out.append(f'<circle cx="{n1[0]}" cy="{n1[1]}" r="11" fill="{c["disc"]}"/>')
    else:
        out.append(f'<circle cx="{ox}" cy="{oy}" r="{R}" fill="{c["disc"]}"/>')
        out.append(f'{route} stroke="{c["inner"]}"/>')
        out.append(f'<circle cx="{n1[0]}" cy="{n1[1]}" r="20" fill="{c["disc"]}" stroke="{c["inner"]}" stroke-width="10"/>')
        out.append(f'<circle cx="{n2[0]}" cy="{n2[1]}" r="18" fill="{c["inner"]}"/>')
        out.append(f'<circle cx="{n3[0]}" cy="{n3[1]}" r="28" fill="{B["violet"]}"/>')
    return "\n".join(out)

def icon_tag(c, small=False):
    """Label Tag: a crossbar-less A (chevron) with a square tag in its counter."""
    w, h = 384, 404
    tp = 96 if not small else 118
    x0, y0 = (512 - w) / 2, (512 + h) / 2
    yi = h * (1 - 2 * tp / w)
    a = tp * 0.2
    pts = [(0, 0), (w / 2 - a, h), (w / 2 + a, h), (w, 0), (w - tp, 0), (w / 2, yi), (tp, 0)]
    d = "M" + " L".join(f"{x0 + px:.1f},{y0 - py:.1f}" for px, py in pts) + " Z"
    s = 74 if not small else 96
    sq = f'<rect x="{256 - s / 2}" y="{y0 - s}" width="{s}" height="{s}" fill="{c["accent"]}"/>'
    return f'<path d="{d}" fill="{c["fg"]}"/>\n{sq}'

def _hex(cx, cy, r):
    return [(cx + r * math.cos(math.radians(a)), cy + r * math.sin(math.radians(a))) for a in (-90, -30, 30, 90, 150, 210)]

def icon_lattice(c, small=False):
    """Lattice Gate: a hex shield, left half in shadow, right half an open lattice, a violet route crossing in."""
    cx, cy, r = 256, 256, 222
    hx = _hex(cx, cy, r)
    hp = "M" + " L".join(f"{x:.1f},{y:.1f}" for x, y in hx) + " Z"
    m = uid("lat")
    sw = 16 if not small else 30
    out = [f'<clipPath id="{m}h"><path d="{hp}"/></clipPath>']
    # left half in shadow
    out.append(f'<rect x="0" y="0" width="{cx}" height="512" fill="{c["disc"]}" clip-path="url(#{m}h)"/>')
    if not small:
        lines = []
        step = 44
        for k in range(-14, 15):
            off = k * step
            lines.append(f"M{cx},{cy + off} L{cx + 300},{cy + off}")
            for ang in (60, 120):
                t = math.radians(ang)
                bx, by = cx + off / math.sin(t) * 0, cy + off / math.cos(math.radians(30))
                lines.append(f"M{cx - 400 * math.cos(t):.1f},{by - 400 * math.sin(t):.1f} L{cx + 400 * math.cos(t):.1f},{by + 400 * math.sin(t):.1f}")
        out.append(f'<clipPath id="{m}r"><rect x="{cx + 4}" y="0" width="{256}" height="512"/></clipPath>')
        op = "" if c["mono"] else ' stroke-opacity="0.45"'
        out.append(f'<g clip-path="url(#{m}h)"><g clip-path="url(#{m}r)"><path d="{" ".join(lines)}" fill="none" stroke="{c["line"]}" stroke-width="5"{op}/></g></g>')
    out.append(f'<path d="{hp}" fill="none" stroke="{c["fg"]}" stroke-width="{sw}" stroke-linejoin="round"/>')
    acc = c["accent"] if not c["mono"] else c["fg"]
    if small:
        out.append(f'<circle cx="{cx + 70}" cy="{cy}" r="46" fill="{acc}"/>')
        return "\n".join(out)
    route = [(cx + 64, 8 + 34), (cx + 64, 150), (cx, 196), (cx, 286)]
    rp = " ".join(f"{x},{y}" for x, y in route)
    halo = c["disc"] if not c["mono"] else "none"
    if c["mono"]:
        # knock the route out of the shadow half so it reads in one color
        out[1] = (f'<mask id="{m}k" maskUnits="userSpaceOnUse" x="0" y="0" width="512" height="512"><rect width="512" height="512" fill="#fff"/>'
                  f'<polyline points="{rp}" fill="none" stroke="#000" stroke-width="40" stroke-linecap="round" stroke-linejoin="round"/>'
                  f'<circle cx="{cx}" cy="286" r="46" fill="#000"/></mask>'
                  f'<rect x="0" y="0" width="{cx}" height="512" fill="{c["disc"]}" clip-path="url(#{m}h)" mask="url(#{m}k)"/>')
    out.append(f'<polyline points="{rp}" fill="none" stroke="{acc}" stroke-width="16" stroke-linecap="round" stroke-linejoin="round"/>')
    out.append(f'<circle cx="{cx + 64}" cy="150" r="17" fill="{acc}"/>')
    out.append(f'<circle cx="{cx}" cy="286" r="32" fill="{acc}"/>')
    return "\n".join(out)

def icon_route(c, small=False):
    """Route Monogram: D and J drawn as chamfered traces; hollow start node, violet end node."""
    sw = 44 if not small else 62
    ty = -14
    D = f"M118,{130 + ty} H268 L338,{200 + ty} V{312 + ty} L268,{382 + ty} H158 V{196 + ty}"
    J = f"M408,{130 + ty} V{344 + ty} L346,{406 + ty} H262"
    out = [f'<path d="{D}" fill="none" stroke="{c["fg"]}" stroke-width="{sw}" stroke-linejoin="miter" stroke-linecap="butt"/>',
           f'<path d="{J}" fill="none" stroke="{c["fg"]}" stroke-width="{sw}" stroke-linejoin="miter" stroke-linecap="butt"/>']
    acc = c["accent"]
    if small:
        out.append(f'<circle cx="236" cy="{406 + ty}" r="40" fill="{acc}"/>')
        return "\n".join(out)
    out.insert(0, f'<circle cx="98" cy="{130 + ty}" r="30" fill="none" stroke="{c["fg"]}" stroke-width="16"/>')
    out[1] = out[1].replace("M118,", "M128,")
    out.append(f'<circle cx="236" cy="{406 + ty}" r="38" fill="{acc}"/>')
    return "\n".join(out)

DIRECTIONS = {
    "eclipse": dict(name="Umbra Eclipse", kind="icon + wordmark", icon=icon_eclipse, font=SG500, tracking=150, lam=False),
    "tag":     dict(name="Label Tag", kind="pure wordmark", icon=icon_tag, font=SG700, tracking=30, lam=True),
    "lattice": dict(name="Lattice Gate", kind="emblem", icon=icon_lattice, font=SG500, tracking=190, lam=False),
    "route":   dict(name="Route Monogram", kind="monogram + wordmark", icon=icon_route, font=SG500, tracking=110, lam=False),
}

# ---------------------------------------------------------------- lockups
def svg(w, h, body, bg=None, title="DARKJUANDER"):
    bgr = f'<rect width="100%" height="100%" fill="{bg}"/>' if bg else ""
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w:.1f} {h:.1f}" width="{w:.0f}" height="{h:.0f}">'
            f'<title>{title}</title>\n{bgr}\n{body}\n</svg>\n')

def wordmark(dname, c, cap_h, x=0, y=0, text="DARKJUANDER"):
    """Returns (svg fragment, width). (x, y) is the baseline start."""
    D = DIRECTIONS[dname]
    f = D["font"]
    size = cap_h / f.cap * f.upm
    tags = []
    custom = {"A": lambda_A(f, tags)} if D["lam"] else None
    d, w = f.text(text, size, x, y, D["tracking"], custom)
    frag = f'<path d="{d}" fill="{c["fg"]}"/>'
    for tx, ty, s in tags:
        frag += f'<rect x="{tx:.2f}" y="{ty:.2f}" width="{s:.2f}" height="{s:.2f}" fill="{c["accent"]}"/>'
    return frag, w

def place_icon(dname, c, x, y, size, small=False):
    s = size / 512
    return f'<g transform="translate({x:.2f},{y:.2f}) scale({s:.5f})">{DIRECTIONS[dname]["icon"](c, small)}</g>'

def lockup_horizontal(dname, c):
    if DIRECTIONS[dname]["kind"] == "pure wordmark":
        cap = 200
        frag, w = wordmark(dname, c, cap, 0, cap)
        return frag, w, cap
    icon = 512
    cap = 150
    gap = 112
    frag_w, w = wordmark(dname, c, cap, icon + gap, icon / 2 + cap / 2)
    return place_icon(dname, c, 0, 0, icon) + frag_w, icon + gap + w, icon

def lockup_stacked(dname, c):
    if DIRECTIONS[dname]["kind"] == "pure wordmark":
        cap = 200
        lead = 290
        _, w1 = wordmark(dname, c, cap, 0, 0, "DARK")
        _, w2 = wordmark(dname, c, cap, 0, 0, "JUANDER")
        W = max(w1, w2)
        f1, _ = wordmark(dname, c, cap, (W - w1) / 2, cap, "DARK")
        f2, _ = wordmark(dname, c, cap, (W - w2) / 2, cap + lead, "JUANDER")
        return f1 + f2, W, cap + lead
    icon = 512
    cap = 104
    gap = 100
    _, w = wordmark(dname, c, cap)
    W = max(icon, w)
    fw, _ = wordmark(dname, c, cap, (W - w) / 2, icon + gap + cap)
    return place_icon(dname, c, (W - icon) / 2, 0, icon) + fw, W, icon + gap + cap

# ---------------------------------------------------------------- export helpers
def write(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)

def png(src, dst, w=None, h=None, bg=None):
    if NO_PNG:
        return
    dst.parent.mkdir(parents=True, exist_ok=True)
    cmd = ["rsvg-convert", str(src), "-o", str(dst)]
    if w: cmd += ["-w", str(w)]
    if h: cmd += ["-h", str(h)]
    if bg: cmd += ["--background-color", bg]
    subprocess.run(cmd, check=True)

ICON_SIZES = [16, 32, 64, 128, 256, 512, 1024]

def build_logos():
    for dname in DIRECTIONS:
        base = ROOT / "logo" / dname
        for v, c in VARIANTS.items():
            write(base / "svg" / f"{dname}-icon-{v}.svg", svg(512, 512, place_icon(dname, c, 0, 0, 512)))
            write(base / "svg" / f"{dname}-icon-small-{v}.svg", svg(512, 512, place_icon(dname, c, 0, 0, 512, small=True)))
            for lay, fn in (("horizontal", lockup_horizontal), ("stacked", lockup_stacked)):
                body, w, h = fn(dname, c)
                pad = round(h * 0.03, 1)  # keeps glyph overshoot from clipping; not the clear-space rule
                write(base / "svg" / f"{dname}-{lay}-{v}.svg",
                      svg(w + 2 * pad, h + 2 * pad, f'<g transform="translate({pad},{pad})">{body}</g>'))
        if NO_PNG:
            continue
        for s in ICON_SIZES:
            src = base / "svg" / (f"{dname}-icon-small-dark.svg" if s <= 32 else f"{dname}-icon-dark.svg")
            srcl = base / "svg" / (f"{dname}-icon-small-light.svg" if s <= 32 else f"{dname}-icon-light.svg")
            png(src, base / "png" / "icon" / f"{dname}-icon-{s}-transparent.png", s, s)
            png(src, base / "png" / "icon" / f"{dname}-icon-{s}-on-dark.png", s, s, N["umbra_950"])
            png(srcl, base / "png" / "icon" / f"{dname}-icon-{s}-light-transparent.png", s, s)
        for lay in ("horizontal", "stacked"):
            for v in VARIANTS:
                src = base / "svg" / f"{dname}-{lay}-{v}.svg"
                for wpx in (512, 1024, 2048):
                    png(src, base / "png" / lay / f"{dname}-{lay}-{v}-{wpx}w.png", wpx)
            for wpx in (1024, 2048):
                png(base / "svg" / f"{dname}-{lay}-dark.svg", base / "png" / lay / f"{dname}-{lay}-dark-on-dark-{wpx}w.png", wpx, None, N["umbra_950"])

def label(text, x, y, size, color, font=None, tracking=0):
    d, w = (font or INTER).text(text, size, x, y, tracking)
    return f'<path d="{d}" fill="{color}"/>', w

def build_comparison():
    W, H = 2400, 1500
    names = list(DIRECTIONS)
    body = [f'<rect width="{W}" height="{H}" fill="{N["lumen_50"]}"/>', f'<rect width="{W/2}" height="{H}" fill="{N["umbra_950"]}"/>',
            f'<rect width="{W}" height="170" fill="{N["umbra_900"]}"/>', f'<rect y="169" width="{W}" height="1" fill="{N["umbra_600"]}"/>']
    t, _ = label("DARKJUANDER logo directions", 60, 90, 40, N["lumen_100"], INTER600); body.append(t)
    t, _ = label("A is recommended. Left half on dark, right half on light. Each row: icon, horizontal lockup, then the small-size mark at 64 px and 32 px.", 60, 136, 22, N["ash_400"]); body.append(t)
    rowh = (H - 190) / 4
    for i, dn in enumerate(names):
        y0 = 190 + i * rowh
        D = DIRECTIONS[dn]
        for side, v, ox in (("dark", "dark", 0), ("light", "light", W / 2)):
            c = VARIANTS[v]
            col = N["lumen_100"] if side == "dark" else N["umbra_950"]
            mut = N["ash_400"] if side == "dark" else N["umbra_500"]
            t, _ = label(f"{'ABCD'[i]}  {D['name']}", ox + 60, y0 + 40, 26, col, INTER600); body.append(t)
            t, _ = label(D["kind"] + ("  (recommended)" if dn == "eclipse" else ""), ox + 60, y0 + 72, 20, mut); body.append(t)
            body.append(place_icon(dn, c, ox + 60, y0 + 92, 200))
            frag, lw, lh = lockup_horizontal(dn, c)
            maxw, maxh = 640, 120
            s = min(maxw / lw, maxh / lh)
            body.append(f'<g transform="translate({ox + 300:.1f},{y0 + 92 + (200 - lh * s) / 2:.1f}) scale({s:.4f})">{frag}</g>')
            body.append(place_icon(dn, c, ox + 990, y0 + 150, 64, small=True))
            body.append(place_icon(dn, c, ox + 1080, y0 + 174, 32, small=True))
            # small-size proof at true pixel size
        if i < 3:
            body.append(f'<rect x="60" y="{y0 + rowh - 6}" width="{W - 120}" height="1" fill="{N["umbra_600"]}" opacity="0.5"/>')
    p = ROOT / "logo/logo-directions.svg"
    write(p, svg(W, H, "\n".join(body), title="DARKJUANDER logo directions"))
    png(p, ROOT / "logo/logo-directions.png", W)

# ---------------------------------------------------------------- OS assets
def grid(w, h, step, color, op):
    d = " ".join([f"M{x},0 V{h}" for x in range(0, w + 1, step)] + [f"M0,{y} H{w}" for y in range(0, h + 1, step)])
    return f'<path d="{d}" stroke="{color}" stroke-width="1" opacity="{op}" fill="none"/>'

def wallpaper(dname, w, h):
    c = VARIANTS["dark"]
    m = uid("wp")
    body = [f'<rect width="{w}" height="{h}" fill="{N["umbra_950"]}"/>', grid(w, h, int(h / 27), N["umbra_800"], 0.5)]
    if dname == "eclipse":
        R = h * 0.40
        ox, oy = w * 0.70, h * 0.53
        lx, ly = ox + h * 0.040, oy - h * 0.030
        gap = h * 0.008
        body.insert(0, f'<defs><mask id="{m}c" maskUnits="userSpaceOnUse" x="0" y="0" width="{w}" height="{h}">'
                       f'<rect width="{w}" height="{h}" fill="#fff"/><circle cx="{ox}" cy="{oy}" r="{R + gap}" fill="#000"/></mask>'
                       f'<filter id="{m}b" x="-50%" y="-50%" width="200%" height="200%"><feGaussianBlur stdDeviation="{h * 0.03:.1f}"/></filter>'
                       f'<radialGradient id="{m}g" cx="{lx / w:.3f}" cy="{ly / h:.3f}" r="0.6"><stop offset="0" stop-color="{B["violet"]}" stop-opacity="0.10"/>'
                       f'<stop offset="1" stop-color="{B["violet"]}" stop-opacity="0"/></radialGradient></defs>')
        body += [f'<rect width="{w}" height="{h}" fill="url(#{m}g)"/>',
                 f'<circle cx="{lx}" cy="{ly}" r="{R}" fill="{B["violet"]}" opacity="0.55" mask="url(#{m}c)" filter="url(#{m}b)"/>',
                 f'<circle cx="{lx}" cy="{ly}" r="{R}" fill="{B["violet"]}" mask="url(#{m}c)"/>',
                 f'<circle cx="{ox}" cy="{oy}" r="{R}" fill="{N["umbra_900"]}"/>']
        k = R / 178
        n1, n2, n3 = (ox - 92 * k, oy + 62 * k), (ox - 6 * k, oy + 18 * k), (ox + 70 * k, oy - 66 * k)
        body.append(f'<g opacity="0.22"><polyline points="{n1[0]:.1f},{n1[1]:.1f} {n2[0]:.1f},{n2[1]:.1f} {n3[0]:.1f},{n3[1]:.1f}" fill="none" '
                    f'stroke="{N["lumen_100"]}" stroke-width="{4 * k:.1f}" stroke-linecap="round"/>'
                    f'<circle cx="{n1[0]:.1f}" cy="{n1[1]:.1f}" r="{7 * k:.1f}" fill="{N["umbra_900"]}" stroke="{N["lumen_100"]}" stroke-width="{3 * k:.1f}"/>'
                    f'<circle cx="{n2[0]:.1f}" cy="{n2[1]:.1f}" r="{6 * k:.1f}" fill="{N["lumen_100"]}"/></g>'
                    f'<circle cx="{n3[0]:.1f}" cy="{n3[1]:.1f}" r="{9 * k:.1f}" fill="{B["violet"]}" opacity="0.8"/>')
    else:
        big = h * 0.62
        body.append(f'<g opacity="0.5">{place_icon(dname, c, w * 0.60, (h - big) / 2, big)}</g>')
    frag, ww = wordmark(dname, dict(c, fg=N["ash_400"]), h * 0.012, w * 0.035, h * 0.955)
    body.append(f'<g opacity="0.8">{frag}</g>')
    return svg(w, h, "\n".join(body), title="DARKJUANDER wallpaper")

def build_os(dname):
    out = ROOT / "wallpapers"
    for w, h in ((3840, 2160), (2560, 1600), (1920, 1080)):
        p = out / f"darkjuander-{w}x{h}.svg"
        write(p, wallpaper(dname, w, h))
        png(p, out / f"darkjuander-{w}x{h}.png", w, h)

    # OpenGraph / social card
    W, H = 1200, 630
    c = VARIANTS["dark"]
    body = [f'<rect width="{W}" height="{H}" fill="{N["umbra_950"]}"/>', grid(W, H, 30, N["umbra_800"], 0.5),
            place_icon(dname, c, 760, 110, 460)]
    frag, lw, lh = lockup_horizontal(dname, c)
    s = 64 / lh if DIRECTIONS[dname]["kind"] != "pure wordmark" else 34 / lh
    body.append(f'<g transform="translate(72,72) scale({s:.4f})">{frag}</g>')
    y = 300
    for line in ("Run AI agents inside", "a boundary you can audit."):
        t, _ = label(line, 72, y, 54, N["lumen_100"], SG500); body.append(t); y += 68
    t, _ = label("Linux workstation OS  /  Fedora bootc base  /  UMBRA core", 72, 520, 20, N["ash_400"], MONO); body.append(t)
    t, _ = label("github.com/Awkward8787/DARKJUANDER", 72, 556, 20, B["violet"], MONO); body.append(t)
    p = ROOT / "web/og-card-1200x630.svg"
    write(p, svg(W, H, "\n".join(body), title="DARKJUANDER social card"))
    png(p, ROOT / "web/og-card-1200x630.png", W, H)

    # favicon: tile + small mark
    tile = f'<rect width="512" height="512" rx="112" fill="{N["umbra_950"]}"/>'
    fav = svg(512, 512, tile + place_icon(dname, c, 40, 40, 432, small=True), title="DARKJUANDER")
    write(ROOT / "web/favicon.svg", fav)
    full = svg(512, 512, tile + place_icon(dname, c, 40, 40, 432), title="DARKJUANDER")
    write(ROOT / "web/app-icon.svg", full)
    if not NO_PNG:
        for s in (16, 32, 48):
            png(ROOT / "web/favicon.svg", ROOT / f"web/.fav-{s}.png", s, s)
        from PIL import Image
        imgs = [Image.open(ROOT / f"web/.fav-{s}.png") for s in (16, 32, 48)]
        imgs[2].save(ROOT / "web/favicon.ico", sizes=[(16, 16), (32, 32), (48, 48)], append_images=imgs[:2])
        for s in (16, 32, 48):
            (ROOT / f"web/.fav-{s}.png").unlink()
        png(ROOT / "web/app-icon.svg", ROOT / "web/apple-touch-icon.png", 180, 180)
        png(ROOT / "web/app-icon.svg", ROOT / "web/icon-192.png", 192, 192)
        png(ROOT / "web/app-icon.svg", ROOT / "web/icon-512.png", 512, 512)
        # OS icon theme sizes (hicolor)
        for s in (16, 24, 32, 48, 64, 128, 256, 512):
            src = ROOT / f"logo/{dname}/svg/{dname}-icon-{'small-' if s <= 32 else ''}dark.svg"
            png(src, ROOT / f"os-icon/hicolor/{s}x{s}/apps/darkjuander.png", s, s)
        (ROOT / "os-icon/hicolor/scalable/apps").mkdir(parents=True, exist_ok=True)
        shutil.copy(ROOT / f"logo/{dname}/svg/{dname}-icon-dark.svg", ROOT / "os-icon/hicolor/scalable/apps/darkjuander.svg")

    # Plymouth
    pl = ROOT / "plymouth/darkjuander"
    pl.mkdir(parents=True, exist_ok=True)
    write(pl / "logo.svg", svg(512, 512, place_icon(dname, c, 0, 0, 512)))
    png(pl / "logo.svg", pl / "logo.png", 256, 256)
    frag, ww = wordmark(dname, c, 40)
    write(pl / "wordmark.svg", svg(ww, 40, frag.replace('d="', 'transform="translate(0,40)" d="', 1) if False else f'<g transform="translate(0,40)">{frag}</g>'))
    png(pl / "wordmark.svg", pl / "wordmark.png", None, 22)
    write(pl / "progress_box.svg", svg(240, 4, f'<rect width="240" height="4" rx="2" fill="{N["umbra_800"]}"/>'))
    write(pl / "progress_bar.svg", svg(240, 4, f'<rect width="240" height="4" rx="2" fill="{B["violet"]}"/>'))
    write(pl / "entry.svg", svg(320, 44, f'<rect x="1" y="1" width="318" height="42" rx="8" fill="{N["umbra_900"]}" stroke="{N["umbra_500"]}" stroke-width="2"/>'))
    write(pl / "bullet.svg", svg(12, 12, f'<circle cx="6" cy="6" r="5" fill="{N["lumen_100"]}"/>'))
    write(pl / "lock.svg", svg(512, 512, place_icon(dname, c, 0, 0, 512, small=True)))
    for n_, w_, h_ in (("progress_box", 240, 4), ("progress_bar", 240, 4), ("entry", 320, 44), ("bullet", 12, 12)):
        png(pl / f"{n_}.svg", pl / f"{n_}.png", w_, h_)
    png(pl / "lock.svg", pl / "lock.png", 40, 40)

    # GRUB
    gr = ROOT / "grub/darkjuander"
    gr.mkdir(parents=True, exist_ok=True)
    gw, gh = 1920, 1080
    gb = [f'<rect width="{gw}" height="{gh}" fill="{N["umbra_950"]}"/>', grid(gw, gh, 40, N["umbra_800"], 0.45),
          place_icon(dname, c, gw / 2 - 90, 150, 180)]
    frag, ww = wordmark(dname, c, 26)
    gb.append(f'<g transform="translate({gw / 2 - ww / 2:.1f},{150 + 180 + 64})">{frag}</g>')
    write(gr / "background.svg", svg(gw, gh, "\n".join(gb), title="DARKJUANDER GRUB background"))
    png(gr / "background.svg", gr / "background.png", gw, gh)
    for nm, fill, stroke in (("select", N["umbra_800"], B["violet"]),):
        for part, body in {
            "c": f'<rect width="8" height="44" fill="{fill}"/>',
            "w": f'<rect width="8" height="44" fill="{fill}"/><rect width="3" height="44" fill="{stroke}"/>',
            "e": f'<rect width="8" height="44" fill="{fill}"/>',
        }.items():
            write(gr / f"{nm}_{part}.svg", svg(8, 44, body))
            png(gr / f"{nm}_{part}.svg", gr / f"{nm}_{part}.png", 8, 44)
    for part, body in {"c": f'<rect width="6" height="6" fill="{N["umbra_700"]}"/>'}.items():
        write(gr / f"slider_{part}.svg", svg(6, 6, body)); png(gr / f"slider_{part}.svg", gr / f"slider_{part}.png", 6, 6)
    for part, body in {"c": f'<rect width="6" height="6" fill="{B["violet"]}"/>'}.items():
        write(gr / f"thumb_{part}.svg", svg(6, 6, body)); png(gr / f"thumb_{part}.svg", gr / f"thumb_{part}.png", 6, 6)
    mk = shutil.which("grub2-mkfont") or shutil.which("grub-mkfont")
    if mk:
        for fnt, sz, nm in (("Inter.ttf", 18, "inter_regular_18"), ("Inter.ttf", 24, "inter_regular_24"), ("JetBrainsMono.ttf", 16, "jetbrains_mono_16")):
            subprocess.run([mk, "-s", str(sz), "-o", str(gr / f"{nm}.pf2"), str(CACHE / ("Inter-opsz14-wght400.ttf" if "Inter" in fnt else "JetBrainsMono-wght400.ttf"))], check=True)
    else:
        print("note: grub-mkfont not found; GRUB .pf2 fonts not generated (see grub/README.md)")

    build_terminal()
    (ROOT / "BUILD_DIRECTION").write_text(dname + "\n")

def build_terminal():
    t = PAL["terminal"]
    a = t["ansi"]
    td = ROOT / "terminal"
    names = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"]
    hdr = "DARKJUANDER terminal colors. Generated by scripts/build.py from src/palette.json. Do not edit by hand."
    x = [f"! {hdr}", f"*.background: {t['background']}", f"*.foreground: {t['foreground']}", f"*.cursorColor: {t['cursor']}"]
    x += [f"*.color{i}: {c}" for i, c in enumerate(a)]
    write(td / "darkjuander.Xresources", "\n".join(x) + "\n")
    s = lambda c: c.lstrip("#")
    foot = [f"# {hdr}", "[colors]", f"background={s(t['background'])}", f"foreground={s(t['foreground'])}",
            f"selection-background={s(t['selection_bg'])}", f"selection-foreground={s(t['selection_fg'])}",
            f"regular0={s(a[0])}"] + [f"regular{i}={s(a[i])}" for i in range(1, 8)] + [f"bright{i}={s(a[i + 8])}" for i in range(8)]
    foot += ["", "[cursor]", f"color={s(t['background'])} {s(t['cursor'])}"]
    write(td / "foot.ini", "\n".join(foot) + "\n")
    kitty = [f"# {hdr}", f"background {t['background']}", f"foreground {t['foreground']}", f"cursor {t['cursor']}",
             f"cursor_text_color {t['background']}", f"selection_background {t['selection_bg']}", f"selection_foreground {t['selection_fg']}",
             f"url_color {a[12]}", f"active_border_color {t['cursor']}", f"inactive_border_color {N['umbra_600']}",
             f"active_tab_background {N['umbra_800']}", f"active_tab_foreground {t['foreground']}",
             f"inactive_tab_background {t['background']}", f"inactive_tab_foreground {N['ash_400']}"]
    kitty += [f"color{i} {c}" for i, c in enumerate(a)]
    kitty += ["font_family JetBrains Mono"]
    write(td / "kitty.conf", "\n".join(kitty) + "\n")
    al = [f"# {hdr}", "[colors.primary]", f'background = "{t["background"]}"', f'foreground = "{t["foreground"]}"',
          "", "[colors.cursor]", f'text = "{t["background"]}"', f'cursor = "{t["cursor"]}"',
          "", "[colors.selection]", f'text = "{t["selection_fg"]}"', f'background = "{t["selection_bg"]}"', "", "[colors.normal]"]
    al += [f'{n} = "{a[i]}"' for i, n in enumerate(names)] + ["", "[colors.bright]"] + [f'{n} = "{a[i + 8]}"' for i, n in enumerate(names)]
    al += ["", "[font]", 'normal = { family = "JetBrains Mono", style = "Regular" }']
    write(td / "alacritty.toml", "\n".join(al) + "\n")
    pal = "[" + ", ".join(f"'{c}'" for c in a) + "]"
    gt = [f"# {hdr}",
          "# Load with:  dconf load /org/gnome/terminal/legacy/profiles:/:d4c1a5e0-7d1f-4c2b-9a3e-6f0b2c8e1d55/ < gnome-terminal.dconf",
          "# Then add the profile id to /org/gnome/terminal/legacy/profiles:/list (see terminal/README.md).",
          "[/]", "visible-name='DARKJUANDER'", "use-theme-colors=false",
          f"background-color='{t['background']}'", f"foreground-color='{t['foreground']}'",
          "bold-color-same-as-fg=true", "cursor-colors-set=true",
          f"cursor-background-color='{t['cursor']}'", f"cursor-foreground-color='{t['background']}'",
          "highlight-colors-set=true", f"highlight-background-color='{t['selection_bg']}'", f"highlight-foreground-color='{t['selection_fg']}'",
          f"palette={pal}", "use-system-font=false", "font='JetBrains Mono 11'"]
    write(td / "gnome-terminal.dconf", "\n".join(gt) + "\n")
    write(td / "palette.txt", "GNOME Terminal palette (paste into Preferences > Profile > Colors, 16 entries, color0..color15):\n" + "\n".join(f"color{i:<2} {c}" for i, c in enumerate(a)) + "\n")
    # preview image of the palette
    rows = []
    for i, c in enumerate(a):
        x0, y0 = 40 + (i % 8) * 120, 120 + (i // 8) * 130
        rows.append(f'<rect x="{x0}" y="{y0}" width="100" height="64" rx="6" fill="{c}" stroke="{N["umbra_600"]}"/>')
        tx, _ = label(f"{i:>2} {c}", x0, y0 + 92, 15, N["ash_400"], MONO); rows.append(tx)
    tx, _ = label("dj audit verify ~/.local/state/dj/audit.log", 40, 60, 22, t["foreground"], MONO); rows.append(tx)
    tx, _ = label("ok", 40, 92, 22, a[2], MONO); rows.append(tx)
    tx, _ = label("warn", 100, 92, 22, a[3], MONO); rows.append(tx)
    tx, _ = label("deny", 184, 92, 22, a[1], MONO); rows.append(tx)
    tx, _ = label("route=mid-local", 268, 92, 22, a[5], MONO); rows.append(tx)
    p = td / "preview.svg"
    write(p, svg(1000, 400, f'<rect width="1000" height="400" fill="{t["background"]}"/>' + "".join(rows), title="DARKJUANDER terminal palette"))
    png(p, td / "preview.png", 1000)

def build_swatches():
    groups = [("Brand", PAL["brand"]), ("Neutrals", PAL["neutral"]), ("Semantic (dark UI)", PAL["semantic_dark"]), ("Semantic (light UI)", PAL["semantic_light"])]
    body = [f'<rect width="1600" height="980" fill="{N["umbra_950"]}"/>']
    y = 70
    for gname, cols in groups:
        t, _ = label(gname, 60, y, 24, N["lumen_100"], INTER600); body.append(t)
        y += 24
        for i, (k, v) in enumerate(cols.items()):
            x = 60 + (i % 10) * 150
            body.append(f'<rect x="{x}" y="{y}" width="130" height="90" rx="8" fill="{v}" stroke="{N["umbra_600"]}"/>')
            t, _ = label(k, x, y + 116, 15, N["ash_200"], INTER); body.append(t)
            t, _ = label(v, x, y + 138, 15, N["ash_400"], MONO); body.append(t)
        y += 200
    p = ROOT / "logo/palette-swatches.svg"
    write(p, svg(1600, 980, "\n".join(body), title="DARKJUANDER palette"))
    png(p, ROOT / "logo/palette-swatches.png", 1600)

if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--direction", default="eclipse", choices=list(DIRECTIONS))
    ap.add_argument("--no-png", action="store_true")
    args = ap.parse_args()
    NO_PNG = args.no_png
    if not NO_PNG and not shutil.which("rsvg-convert"):
        sys.exit("rsvg-convert is required for PNG export (or pass --no-png)")
    build_logos()
    build_comparison()
    build_swatches()
    build_os(args.direction)
    print(f"done. OS assets use direction: {args.direction}")
