"""Builds monadeck-icons.otf: Monadeck's own glyphs, the ones Phosphor lacks
(left / right VR controllers for the battery chips: Index-style for Monado,
Quest-style for WiVRn).

They're drawn the way Phosphor draws: centrelines on a 256 grid with a
16-unit round stroke, outlined by skia-pathops, in a tiny CFF font on
Phosphor's metrics (1024 upem, ascent 960, descent 64), so they sit, size and
space exactly like its glyphs. Code points are in plane 15's private use area,
clear of Phosphor's; `gfx::glyph` in the overlay names them.

    pip install fonttools skia-pathops
    python3 build_icons.py        # rewrites monadeck-icons.otf next to it
"""
import math
import os

import pathops
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.t2CharStringPen import T2CharStringPen
from fontTools.svgLib.path import parse_path

STROKE = 16.0
K = 0.5522847498  # cubic handle length for a quarter circle


def svg(d):
    p = pathops.Path()
    parse_path(d, p.getPen())
    return p


def ellipse(cx, cy, rx, ry, rot=0.0):
    c, s = math.cos(math.radians(rot)), math.sin(math.radians(rot))

    def t(x, y):
        return (cx + x * c - y * s, cy + x * s + y * c)

    p = pathops.Path()
    p.moveTo(*t(rx, 0))
    p.cubicTo(*t(rx, K * ry), *t(K * rx, ry), *t(0, ry))
    p.cubicTo(*t(-K * rx, ry), *t(-rx, K * ry), *t(-rx, 0))
    p.cubicTo(*t(-rx, -K * ry), *t(-K * rx, -ry), *t(0, -ry))
    p.cubicTo(*t(K * rx, -ry), *t(rx, -K * ry), *t(rx, 0))
    p.close()
    return p


def union(paths):
    out = pathops.Path()
    for p in paths:
        out = pathops.op(out, p, pathops.PathOp.UNION)
    return out


def capsule(x0, y0, x1, y1, r0, r1):
    """A tapered capsule between two centres."""
    a = math.atan2(y1 - y0, x1 - x0) + math.pi / 2
    c, s = math.cos(a), math.sin(a)
    quad = pathops.Path()
    quad.moveTo(x0 + c * r0, y0 + s * r0)
    quad.lineTo(x1 + c * r1, y1 + s * r1)
    quad.lineTo(x1 - c * r1, y1 - s * r1)
    quad.lineTo(x0 - c * r0, y0 - s * r0)
    quad.close()
    return union([ellipse(x0, y0, r0, r0), ellipse(x1, y1, r1, r1), quad])


def stroked(p):
    q = pathops.Path()
    q.addPath(p)
    q.stroke(STROKE, pathops.LineCap.ROUND_CAP, pathops.LineJoin.ROUND_JOIN, 4.0)
    q.convertConicsToQuads()
    return q


def centred(p):
    """Centred on the grid, like Phosphor's glyphs."""
    x0, y0, x1, y1 = p.bounds
    return p.transform(1, 0, 0, 1, 128 - (x0 + x1) / 2, 128 - (y0 + y1) / 2)


def mirrored(p):
    return p.transform(-1, 0, 0, 1, 256, 0)


def quest_right():
    """A Quest-style controller for the right hand: head + tilted grip outlined
    as one, the tracking ring rising outwards behind the head, a thumbstick."""
    body = union([ellipse(108, 118, 40, 30), capsule(104, 124, 92, 214, 26, 22)])
    ring = pathops.op(stroked(ellipse(156, 82, 58, 44, -30)), body, pathops.PathOp.DIFFERENCE)
    return centred(union([stroked(body), ring, ellipse(106, 116, 11, 11)]))


def index_right():
    """An Index-style controller for the right hand: a tall grip under a head
    leaning forward, the hand strap bowing out on the outer side, a thumbstick."""
    body = svg(
        "M104,40 C104,28 114,22 126,24 L152,30 C166,34 172,46 168,60 L150,112 "
        "L150,212 C150,226 142,234 128,234 C114,234 106,226 106,212 L106,110 Z"
    )
    strap = svg("M168,60 C206,86 208,184 150,214")
    return centred(union([stroked(body), stroked(strap), ellipse(132, 52, 10, 10)]))


GLYPHS = {
    0xF0000: ("quest-left", lambda: mirrored(quest_right())),
    0xF0001: ("quest-right", quest_right),
    0xF0002: ("index-left", lambda: mirrored(index_right())),
    0xF0003: ("index-right", index_right),
}


def build(out):
    order = [".notdef"] + [name for name, _ in GLYPHS.values()]
    charstrings = {".notdef": T2CharStringPen(1024, None).getCharString()}
    for name, draw in GLYPHS.values():
        pen = T2CharStringPen(1024, None)
        # 256 grid, y down -> font units, y up, baseline 64 units above the bottom.
        draw().transform(4, 0, 0, -4, 0, 960).draw(pen)
        charstrings[name] = pen.getCharString()
    fb = FontBuilder(1024, isTTF=False)
    fb.setupGlyphOrder(order)
    fb.setupCharacterMap({cp: name for cp, (name, _) in GLYPHS.items()})
    fb.setupCFF("MonadeckIcons-Regular", {"FullName": "Monadeck Icons"}, charstrings, {})
    fb.setupHorizontalMetrics({name: (1024, 0) for name in order})
    fb.setupHorizontalHeader(ascent=960, descent=-64)
    fb.setupOS2(sTypoAscender=960, sTypoDescender=-64, usWinAscent=960, usWinDescent=64)
    fb.setupNameTable({"familyName": "Monadeck Icons", "styleName": "Regular"})
    fb.setupPost()
    fb.save(out)


if __name__ == "__main__":
    build(os.path.join(os.path.dirname(os.path.abspath(__file__)), "monadeck-icons.otf"))
