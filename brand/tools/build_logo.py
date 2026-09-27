"""Build stretto's logo files in brand/logo/ from one set of constants.

    python3 brand/tools/build_logo.py

The mark is a stretto of three entries: the same bar (the subject) enters
three times, each entry before the last one ends, and each sooner than the
one before (offsets of 6, then 4, on a 32-unit grid). The first entry is ink,
the agent's own call; the two that overlap it are petrol, the reads that run
ahead of it.

The wordmark is "stretto" in Inter Display SemiBold (Inter 4.001 at wght 600,
opsz 32), shaped with HarfBuzz (Inter's kerning joins the crossbars of "tt"),
tracked tighter, and converted to outlines, so no font is needed to show it.
It is read from brand/fonts/Inter-Variable.subset.woff2 (SIL OFL 1.1).
PNG and ICO files come from brand/tools/render_icons.mjs.
"""

import io
from pathlib import Path

import uharfbuzz as hb
from fontTools.pens.boundsPen import BoundsPen
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

BRAND = Path(__file__).resolve().parent.parent
OUT = BRAND / "logo"
FONT = BRAND / "fonts" / "Inter-Variable.subset.woff2"

# Colors: brand constants from brand/tokens.css.
INK = "#141c1e"
PAPER = "#f8fbfb"
PETROL = "#02767b"
PETROL_LIGHT = "#039298"
PETROL_DARK = "#33c0c7"

# The mark, on a 32-unit grid: (x, y, width, height) of each entry.
BARS = [(1, 2.5, 20, 7), (7, 12.5, 20, 7), (11, 22.5, 20, 7)]
RADIUS = 1.5

# The favicon tile, on the same grid. Every edge is on an even unit, so the
# bars fall on whole pixels at 16 px and at 32 px.
TILE_RADIUS = 7
TILE_BARS = [(4, 6, 14, 4), (10, 14, 14, 4), (14, 22, 14, 4)]
TILE_BAR_RADIUS = 1
APP_ICON_SCALE = 0.8

WORD = "stretto"
WGHT, OPSZ = 600, 32
TRACKING = -0.02  # em, added after each letter but the last
T_HEIGHT = 1344  # the top of Inter's "t", in font units (2048 per em)

LOCKUP_SCALE = 1.0  # the mark's bars span this many "t" heights
LOCKUP_GAP = 0.42  # the space between mark and wordmark, in mark heights

TITLE = "stretto"


def fmt(v: float) -> str:
    s = f"{v:.2f}".rstrip("0").rstrip(".")
    return "0" if s in ("-0", "") else s


def bars_svg(bars, colors, radius, dx=0.0, dy=0.0, scale=1.0) -> str:
    out = []
    for (x, y, w, h), color in zip(bars, colors):
        out.append(
            f'<rect x="{fmt(dx + x * scale)}" y="{fmt(dy + y * scale)}" width="{fmt(w * scale)}" '
            f'height="{fmt(h * scale)}" rx="{fmt(radius * scale)}" fill="{color}"/>'
        )
    return "".join(out)


def svg(width: float, height: float, body: str, label: str = TITLE) -> str:
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{fmt(width)}" height="{fmt(height)}" '
        f'viewBox="0 0 {fmt(width)} {fmt(height)}" role="img" aria-label="{label}">'
        f"<title>{label}</title>{body}</svg>\n"
    )


def wordmark_path():
    """The wordmark's outline as one SVG path, with its bounds, in font units."""
    font = TTFont(FONT)
    font.flavor = None
    data = io.BytesIO()
    font.save(data)
    hb_font = hb.Font(hb.Face(hb.Blob(data.getvalue())))
    hb_font.set_variations({"wght": WGHT, "opsz": OPSZ})
    buf = hb.Buffer()
    buf.add_str(WORD)
    buf.guess_segment_properties()
    hb.shape(hb_font, buf, {"kern": True, "liga": True, "calt": True})

    static = instancer.instantiateVariableFont(TTFont(FONT), {"wght": WGHT, "opsz": OPSZ})
    glyphs = static.getGlyphSet()
    upm = static["head"].unitsPerEm
    pen = SVGPathPen(glyphs, ntos=lambda v: fmt(v))
    bounds = BoundsPen(glyphs)
    x = 0.0
    infos = list(zip(buf.glyph_infos, buf.glyph_positions))
    for i, (info, pos) in enumerate(infos):
        name = static.getGlyphName(info.codepoint)
        # Flip y (fonts are y-up) and move to the pen position.
        t = (1, 0, 0, -1, x + pos.x_offset, -pos.y_offset)
        glyphs[name].draw(TransformPen(pen, t))
        glyphs[name].draw(TransformPen(bounds, t))
        x += pos.x_advance + (TRACKING * upm if i < len(infos) - 1 else 0)
    x0, y0, x1, y1 = bounds.bounds
    return pen.getCommands(), (x0, y0, x1, y1), upm, static["OS/2"].sxHeight


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)

    # The mark.
    (OUT / "stretto-mark.svg").write_text(svg(32, 32, bars_svg(BARS, [INK, PETROL_LIGHT, PETROL_LIGHT], RADIUS)))
    (OUT / "stretto-mark-dark.svg").write_text(svg(32, 32, bars_svg(BARS, [PAPER, PETROL_DARK, PETROL_DARK], RADIUS)))

    # The favicon: a petrol tile with paper bars, legible on light and dark tab bars.
    tile = f'<rect width="32" height="32" rx="{TILE_RADIUS}" fill="{PETROL}"/>'
    (OUT / "favicon.svg").write_text(svg(32, 32, tile + bars_svg(TILE_BARS, [PAPER] * 3, TILE_BAR_RADIUS)))
    # App icons are full-bleed squares, which platforms round or mask. The bars are
    # scaled to 80% about the center, inside the maskable safe zone (a circle of 40%).
    square = f'<rect width="32" height="32" fill="{PETROL}"/>'
    k = APP_ICON_SCALE
    (OUT / "app-icon.svg").write_text(
        svg(32, 32, square + bars_svg(TILE_BARS, [PAPER] * 3, TILE_BAR_RADIUS, dx=16 * (1 - k), dy=16 * (1 - k), scale=k))
    )

    # The wordmark, at a nominal 64 px font size.
    d, (x0, y0, x1, y1), upm, xh = wordmark_path()
    size = 64
    s = size / upm
    pad = 0
    w, h = (x1 - x0) * s + 2 * pad, (y1 - y0) * s + 2 * pad
    group = lambda color: (
        f'<path fill="{color}" transform="translate({fmt(pad - x0 * s)} {fmt(pad - y0 * s)}) scale({s:.6f})" d="{d}"/>'
    )
    (OUT / "stretto-wordmark.svg").write_text(svg(w, h, group(INK)))
    (OUT / "stretto-wordmark-dark.svg").write_text(svg(w, h, group(PAPER)))

    # The lockup: the mark's bars span the wordmark's "t", from its top to the
    # baseline, and a gap of LOCKUP_GAP mark heights separates the two.
    t_top, baseline = -T_HEIGHT * s, 0.0  # y of the t's top and of the baseline, at this size
    bars_top, bars_bottom = BARS[0][1], BARS[-1][1] + BARS[-1][3]
    mark_ink_h = bars_bottom - bars_top
    target = (baseline - t_top) * LOCKUP_SCALE
    k = target / mark_ink_h
    mark_y = baseline - bars_bottom * k
    mark_left = min(x for x, _, _, _ in BARS)
    mark_right = max(x + w for x, _, w, _ in BARS)
    mark_dx = -mark_left * k  # the first entry's left edge at x = 0
    gap = LOCKUP_GAP * target
    word_x = (mark_right - mark_left) * k + gap - x0 * s
    top = min(mark_y + bars_top * k, y0 * s)
    bottom = max(mark_y + bars_bottom * k, y1 * s)
    lw = word_x + x1 * s
    lh = bottom - top

    def lockup(ink, accent, word):
        return svg(
            lw, lh,
            bars_svg(BARS, [ink, accent, accent], RADIUS, dx=mark_dx, dy=mark_y - top, scale=k)
            + f'<path fill="{word}" transform="translate({fmt(word_x)} {fmt(-top)}) scale({s:.6f})" d="{d}"/>',
        )

    (OUT / "stretto-lockup.svg").write_text(lockup(INK, PETROL_LIGHT, INK))
    (OUT / "stretto-lockup-dark.svg").write_text(lockup(PAPER, PETROL_DARK, PAPER))
    for f in sorted(OUT.glob("*.svg")):
        print(f"{f.relative_to(BRAND.parent)}: {f.stat().st_size} bytes")


if __name__ == "__main__":
    main()
