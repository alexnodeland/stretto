"""Subset the brand's two typefaces into brand/fonts/.

    python3 brand/tools/subset_fonts.py --inter InterVariable.woff2 --mono-dir DIR

--inter is InterVariable.woff2 (Inter 4.001, npm `inter-ui@4.1.1`, file
`variable/InterVariable.woff2`); --mono-dir holds JetBrains Mono 2.242's static
woff2 files (npm `jetbrains-mono@1.0.6`, `fonts/webfonts/`). Both are licensed
under the SIL Open Font License 1.1 with no Reserved Font Name; the licenses are
in brand/fonts/. The subsets keep every OpenType feature and Inter's `opsz` and
`wght` axes, and only the characters the brand's pages and videos use.
"""

import argparse
from pathlib import Path

from fontTools import subset
from fontTools.ttLib import TTFont

TEXT_RANGES = [
    (0x0020, 0x007E), (0x00A0, 0x00FF), (0x0131, 0x0131), (0x0152, 0x0153),
    (0x02C6, 0x02C6), (0x02DC, 0x02DC), (0x0391, 0x03A9), (0x03B1, 0x03C9),
    (0x2010, 0x2027), (0x2030, 0x203A), (0x2044, 0x2044), (0x2070, 0x2079),
    (0x2080, 0x2089), (0x20AC, 0x20AC), (0x2122, 0x2122), (0x2190, 0x2199),
    (0x21A9, 0x21AA), (0x21B5, 0x21B5), (0x2212, 0x2212), (0x2215, 0x2215),
    (0x2219, 0x2219), (0x221E, 0x221E), (0x2248, 0x2248), (0x2260, 0x2260),
    (0x2264, 0x2265), (0x22C5, 0x22C5), (0x2713, 0x2713), (0x2715, 0x2715),
    (0x2717, 0x2717),
]
# The terminal also draws boxes, blocks and a cursor.
MONO_RANGES = TEXT_RANGES + [(0x2500, 0x257F), (0x2580, 0x259F), (0x25A0, 0x25FF)]


def codepoints(ranges):
    return [c for lo, hi in ranges for c in range(lo, hi + 1)]


def make_subset(src: Path, dst: Path, ranges) -> None:
    font = TTFont(src)
    options = subset.Options()
    options.layout_features = ["*"]
    options.name_IDs = ["*"]
    options.name_languages = ["*"]
    options.notdef_outline = True
    options.flavor = "woff2"
    sub = subset.Subsetter(options)
    cmap = font.getBestCmap()
    sub.populate(unicodes=[c for c in codepoints(ranges) if c in cmap])
    sub.subset(font)
    font.flavor = "woff2"
    font.save(dst)
    print(f"{dst}: {dst.stat().st_size / 1024:.1f} KiB, {len(font.getGlyphOrder())} glyphs")


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--inter", required=True, type=Path)
    ap.add_argument("--mono-dir", required=True, type=Path)
    ap.add_argument("--out", default=Path(__file__).resolve().parent.parent / "fonts", type=Path)
    args = ap.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    make_subset(args.inter, args.out / "Inter-Variable.subset.woff2", TEXT_RANGES)
    for weight in ("Regular", "Bold"):
        make_subset(args.mono_dir / f"JetBrainsMono-{weight}.woff2",
                    args.out / f"JetBrainsMono-{weight}.subset.woff2", MONO_RANGES)


if __name__ == "__main__":
    main()
