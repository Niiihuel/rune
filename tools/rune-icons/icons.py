"""RUNE's icon set, drawn on a 24 px grid and written as fill-only SVG.

Metrics follow Discord's icon system (24 px view box shown at 12-32 px,
2 px lines with round ends, 3 px corners, 2 px gap around a badge); the
shapes are built from primitives with RUNE's own coordinates, never from
Discord's paths (docs/ARTWORK.md). Needs skia-pathops and picosvg:

    nix shell --impure --expr 'with import <nixpkgs> {}; python3.withPackages (p: [ p.skia-pathops p.picosvg ])' \
      -c python3 tools/rune-icons/icons.py assets/glyphs
"""

import math
import os
import sys

import pathops
from picosvg.svg_pathops import skia_path
from picosvg.svg_types import SVGPath

LINE = 2.0  # stroke weight of every line
GAP = 2.0  # clearance between a badge or slash and the shape it cuts


class S:
    """A filled shape; | unites, - cuts, & intersects."""

    def __init__(self, path):
        self.p = path

    def _op(self, other, op):
        return S(pathops.op(self.p, other.p, op))

    def __or__(self, other):
        return self._op(other, pathops.PathOp.UNION)

    def __sub__(self, other):
        return self._op(other, pathops.PathOp.DIFFERENCE)

    def __and__(self, other):
        return self._op(other, pathops.PathOp.INTERSECTION)

    def outline(self, width):
        p = pathops.Path()
        p.addPath(self.p)
        p.stroke(width, pathops.LineCap.ROUND_CAP, pathops.LineJoin.ROUND_JOIN, 4)
        p.convertConicsToQuads()
        return S(p)

    def grow(self, n):
        return self | self.outline(2 * n)

    def shrink(self, n):
        return self - self.outline(2 * n)

    def round(self, r):
        """Round the convex corners to radius r."""
        return self.shrink(r).grow(r)

    def hollow(self, w=LINE):
        """The shape's own outline, w thick and inside its silhouette."""
        return self - self.shrink(w)

    def transform(self, a, b, c, d, e, f):
        return S(self.p.transform(a, b, c, d, e, f))

    def rotate(self, deg, cx=12, cy=12):
        r = math.radians(deg)
        co, si = math.cos(r), math.sin(r)
        return self.transform(
            co, si, -si, co, cx - co * cx + si * cy, cy - si * cx - co * cy
        )

    def mirror(self, axis=12):
        return self.transform(-1, 0, 0, 1, 2 * axis, 0)


def path(d):
    """A filled path in SVG syntax."""
    p = SVGPath(d=d).arcs_to_cubics()
    return S(skia_path(p.as_cmd_seq(), "nonzero"))


def line(d, w=LINE):
    """A stroked centerline with round ends and joins."""
    p = SVGPath(d=d).arcs_to_cubics()
    sp = skia_path(p.as_cmd_seq(), "nonzero")
    sp.stroke(w, pathops.LineCap.ROUND_CAP, pathops.LineJoin.ROUND_JOIN, 4)
    sp.convertConicsToQuads()
    return S(sp)


def circle(cx, cy, r):
    return path(f"M{cx - r} {cy}a{r} {r} 0 1 0 {2 * r} 0a{r} {r} 0 1 0 {-2 * r} 0Z")


def ring(cx, cy, r, w=LINE):
    return circle(cx, cy, r + w / 2) - circle(cx, cy, r - w / 2)


def rect(x, y, w, h, r=0):
    if r == 0:
        return path(f"M{x} {y}h{w}v{h}h{-w}Z")
    return path(
        f"M{x + r} {y}h{w - 2 * r}a{r} {r} 0 0 1 {r} {r}v{h - 2 * r}a{r} {r} 0 0 1 {-r} {r}"
        f"h{-(w - 2 * r)}a{r} {r} 0 0 1 {-r} {-r}v{-(h - 2 * r)}a{r} {r} 0 0 1 {r} {-r}Z"
    )


def cut(shape, mark, gap=GAP):
    """Lay `mark` over `shape`, clearing `gap` around it."""
    return (shape - mark.grow(gap)) | mark


def badge(shape, mark):
    """A badge in the bottom right corner (circle of radius 5 at 19,19)."""
    return cut(shape, mark)


PLUS_BADGE = line("M19 15v8M15 19h8")


# --- the set ---------------------------------------------------------------

ICONS = {}


def icon(name):
    def register(fn):
        ICONS[name] = fn
        return fn

    return register


# --- lines and navigation -----------------------------------------------------


@icon("add")
def _():
    return line("M12 3v18M3 12h18")


@icon("minus")
def _():
    return line("M3 12h18")


@icon("close")
def _():
    return line("M4 4l16 16M20 4 4 20")


@icon("close-bold")
def _():
    return line("M4 4l16 16M20 4 4 20", w=3)


@icon("check")
def _():
    return line("M3 12l6 6 12-12")


@icon("chevron-down")
def _():
    return line("M6 10l6 6 6-6")


@icon("chevron-up")
def _():
    return line("M6 14l6-6 6 6")


@icon("chevron-right")
def _():
    return line("M10 6l6 6-6 6")


def arrow_left():
    return line("M4 12h16M12 4l-8 8 8 8")


@icon("arrow-left")
def _():
    return arrow_left()


@icon("arrow-right")
def _():
    return arrow_left().mirror()


@icon("arrow-up")
def _():
    return arrow_left().rotate(90)


@icon("arrow-down")
def _():
    return arrow_left().rotate(-90)


def reply():
    return line("M3 8h8a8 8 0 0 1 8 8v4M8 3 3 8l5 5")


@icon("reply")
def _():
    return reply()


@icon("forward")
def _():
    return reply().mirror()


@icon("external-link")
def _():
    return line("M7 5h12v12M19 5 5 19")


@icon("refresh")
def _():
    half = line("M6.34 5.01A9 9 0 0 1 19.79 7.5M15 9h6V3")
    return half | half.rotate(180)


@icon("more")
def _():
    return circle(4, 12, 2) | circle(12, 12, 2) | circle(20, 12, 2)


@icon("menu")
def _():
    return line("M2 5h20M2 12h20M2 19h20")


@icon("drag-handle")
def _():
    dots = circle(8, 4, 2)
    for x, y in ((8, 12), (8, 20), (16, 4), (16, 12), (16, 20)):
        dots = dots | circle(x, y, 2)
    return dots


def quarter(corner):
    return corner | corner.rotate(90) | corner.rotate(180) | corner.rotate(270)


@icon("fullscreen")
def _():
    return quarter(line("M3 9V6a3 3 0 0 1 3-3h3"))


@icon("collapse")
def _():
    return quarter(line("M3 9h3a3 3 0 0 0 3-3V3"))


@icon("hash")
def _():
    return (
        rect(7, 2, 3, 20, 0.75)
        | rect(14, 2, 3, 20, 0.75)
        | rect(2, 7, 20, 3, 0.75)
        | rect(2, 14, 20, 3, 0.75)
    )


@icon("at")
def _():
    # Discord's @ is a type glyph, a little heavier than the 2 px line.
    w = 2.3
    return ring(11.6, 12, 3.9, w) | line(
        "M15.6 7.6v5.6a2.6 2.6 0 0 0 5.2 0V12a8.9 8.9 0 1 0-3.4 7", w
    )


@icon("code")
def _():
    # Flat ends cut upright at x = 10, as Discord's brackets.
    left = line("M11 4.9 2.65 12 11 19.1", w=2.3) & rect(0, 0, 10, 24)
    return left.round(0.3) | left.round(0.3).mirror()


@icon("sliders")
def _():
    rails = line("M3 5h18M3 12h18M3 19h18")
    knobs = circle(10.5, 5, 2.5) | circle(15.5, 12, 2.5) | circle(10.5, 19, 2.5)
    return cut(rails, knobs)


@icon("filter")
def _():
    return line("M4 8.5h16M7 13h10M10 17.5h4")


@icon("send")
def _():
    dart = path("M2.6 0.5 24.7 12 2.6 23.5 7.2 12Z").round(1.2)
    return dart - path("M5 9.7 14.3 11.4A.6.6 0 0 1 14.3 12.6L5 14.3Z")


@icon("paperclip")
def _():
    return line(
        "M13.39 19.59 19.99 12.99A6 6 0 0 0 11.51 4.51L3.25 12.75"
        "A4.24 4.24 0 0 0 9.25 18.75L15.75 12.25A2.12 2.12 0 0 0 12.75 9.25L9.5 12.5"
    )


@icon("search")
def _():
    return ring(10, 10, 8) | line("M16 16l5 5")


# --- status and objects -------------------------------------------------------


def disc():
    """The full 22 px disc that status and face icons are cut from."""
    return circle(12, 12, 11)


def along(shape, ox, oy, deg):
    """Place a shape drawn on a local frame (y down the axis) at ox,oy, turned deg."""
    r = math.radians(deg)
    co, si = math.cos(r), math.sin(r)
    return shape.transform(co, si, -si, co, ox, oy)


def taper_bar(top, bottom, y0, y1):
    """An exclamation bar: top wide, bottom narrow, rounded."""
    return path(
        f"M{12 - top / 2} {y0}H{12 + top / 2}L{12 + bottom / 2} {y1}H{12 - bottom / 2}Z"
    ).round(0.9)


@icon("info-circle")
def _():
    stem = line("M11.85 11.2l-1 4.7a1.8 1.8 0 0 0 2.8 1.85l.25-.17")
    return disc() - circle(12.3, 7.3, 1.3) - stem


@icon("check-circle")
def _():
    return disc() - line("M7 13l3 3 7-7")


@icon("close-circle")
def _():
    return disc() - line("M8 8l8 8M16 8l-8 8")


@icon("error-circle")
def _():
    return disc() - taper_bar(2.9, 2.1, 6, 15.1) - circle(12, 17.75, 1.25)


@icon("warning-triangle")
def _():
    tri = path("M12 3.4 21.6 19.6H2.4Z").grow(1.4)
    return tri - taper_bar(2.9, 2.1, 7, 15.1) - circle(12, 17.75, 1.25)


@icon("circle")
def _():
    return ring(12, 12, 8)


@icon("clock")
def _():
    return disc() - line("M12 5v7l3 3")


@icon("history")
def _():
    arc = line("M4.6 7.7A8.6 8.6 0 1 1 3.4 12")
    head = line("M3.6 3.6v4.6h4.6")
    hands = line("M12 7.5V12l3 2.5")
    return arc | head | hands


def bell():
    body = path(
        "M12 1.6a2 2 0 0 1 1.95 1.55A6.9 6.9 0 0 1 19 9.6v2.1l1.35 1.5a3 3 0 0 1 .65 1.85v.5"
        "c0 .55-.3 1.05-.8 1.3C18.8 17.5 15.95 18.4 12 18.4s-6.8-.9-8.2-1.55A1.45 1.45 0 0 1 3 15.55v-.5"
        "a3 3 0 0 1 .65-1.85L5 11.7V9.6a6.9 6.9 0 0 1 5.05-6.45A2 2 0 0 1 12 1.6Z"
    )
    clapper = path("M9 19.9a24 24 0 0 0 6 0 3 3 0 0 1-6 0Z")
    return body | circle(12, 3, 2) | clapper


@icon("bell")
def _():
    return bell()


@icon("bell-slash")
def _():
    return cut(bell(), line("M2 22 22 2"), gap=1.6)


@icon("lock")
def _():
    keyhole = circle(12, 14, 2) | rect(11, 14, 2, 5, 1)
    return (rect(3, 9, 18, 14, 3) - keyhole) | line("M8 10V6a4 4 0 0 1 8 0v4")


@icon("trash")
def _():
    lid = rect(3, 3, 18, 2, 0.75) | rect(9, 1, 6, 3, 0.75)
    can = path("M4 7H20L19.2 20.2A3 3 0 0 1 16.2 23H7.8A3 3 0 0 1 4.8 20.2Z").round(0.9)
    slits = rect(9, 11, 2, 8, 1) | rect(13, 11, 2, 8, 1)
    return lid | (can - slits)


@icon("edit")
def _():
    # A broad body along the up-right diagonal with a chisel tip, and the
    # eraser 2.2 apart from its top.
    body = path("M3 16.2 11.9 6.3 17.75 12 8.5 20.6 2.7 21.6Z").round(1).grow(0.4)
    eraser = rect(13.67, 3.73, 8.26, 4.87, 1.9).rotate(45, 17.8, 6.15)
    return body | eraser


@icon("copy")
def _():
    front = rect(6, 6, 16, 16, 4) - rect(14, -5, 15, 19, 5)
    fold = path("M16 6.2 21.8 12H19A3 3 0 0 1 16 9Z").round(0.4)
    back = line("M3 15V10a7 7 0 0 1 7-7h5")
    return front | fold | back


@icon("pin")
def _():
    # Upright pushpin about 12,12 (cap up), then turned so the needle points
    # down left. Closing then opening fillets the joints and rounds the ends.
    cap = rect(4.8, 2.9, 14.4, 3.5, 1.5)
    neck = rect(7.75, 5, 8.5, 6)
    flange = path("M7.75 10.3 4.7 13.2V16.3H19.3V13.2L16.25 10.3Z")
    head = (cap | neck | flange).grow(0.8).shrink(0.8).round(0.9)
    needle = path("M9.9 18H14.1L12 25Z").round(0.6)
    return (head | needle).rotate(45)


def heart():
    lobes = circle(6.75, 8.75, 5.75) | circle(17.25, 8.75, 5.75)
    lower = path("M1 8.75C1 15 9.3 20.3 12 21.9 14.7 20.3 23 15 23 8.75Z")
    # Fill the cusp between the lobes, then soften the bottom point.
    return (lobes | lower).grow(2).shrink(2).round(0.9)


@icon("heart-fill")
def _():
    return heart()


@icon("heart")
def _():
    return heart().hollow()


@icon("flag")
def _():
    pole = line("M3 2v19")
    cloth = path(
        "M4 3 13.2 1.45A1.6 1.6 0 0 1 15 3V5L21.1 3.95A1.6 1.6 0 0 1 23 5.5V13.4A1.7 1.7 0 0 1 21.6 15.1L12.5 16.6A1.6 1.6 0 0 1 11 15.4 5 5 0 0 0 12.7 13.2L4 14.6Z"
    )
    return pole | cloth.round(0.3)


@icon("target")
def _():
    # A ball (r 8.25 at 14.75,14.75): a pentagon and five panels around it,
    # split at the pentagon's corners; a flame sweeps past its top left.
    c = 14.75
    corners = [
        (c + 3.98 * math.cos(math.radians(a)), c + 3.98 * math.sin(math.radians(a)))
        for a in (-90, -18, 54, 126, 198)
    ]
    pent = path("M" + " ".join(f"{x} {y}" for x, y in corners) + "Z")
    seams = line(
        "".join(f"M{c} {c}L{c + 3 * (x - c)} {c + 3 * (y - c)}" for x, y in corners),
        w=1.9,
    )
    panels = circle(c, c, 8.25) - pent.grow(2) - seams
    # A crescent through its two tips, thickest to the top left, and three tongues.
    band = circle(12.28, 11.77, 9.33) - circle(15.18, 15.03, 10.99)
    # Three blades: one over the top to the right, one to the top left
    # corner, one down the left side.
    blades = (
        path("M11 1Q15.2 1.4 18.6 3.4 18.6 4.6 17.8 4.6Q13.8 4.2 10 5.4Z")
        | path("M1.2 1Q2.6 5.4 6.2 7.6L8.4 5Q4 4.4 1.2 1Z")
        | path("M1 7Q1.4 13 4.3 16.7L5.4 14.2Q4.4 10.6 5.2 8.8 2.6 8.8 1 7Z")
    )
    return panels | pent.round(0.6) | ((band | blades) - circle(c, c, 10.4)).round(0.4)


@icon("bolt")
def _():
    return path("M15.2 1.2 3.6 11.6H10L7.9 22.8 20.4 12.2H14Z").round(0.9)


@icon("globe")
def _():
    # Two seas between the land: one to the east, a wedge to the south west.
    east = path("M13 0H24V18.5H17L12 15.8V12H8V9.5L11 7V4Z")
    land = (
        circle(11.8, 4.3, 2.8)
        | circle(8.5, 7.6, 2.4)
        | circle(12, 15.8, 3.8)
        | circle(17, 18.6, 2.4)
    )
    west = path("M0 10.25H3.1L8.1 15.2 10.8 17.8V24H0Z") - circle(10.9, 15.1, 2.8)
    seas = circle(12, 12, 9) & ((east - land) | west)
    return circle(12, 12, 11) - seas.round(0.3)


@icon("lightbulb")
def _():
    bulb = path(
        "M12 1C16.4 1 20 4.4 20 8.6 20 11.2 19 12.6 18.1 13.9 17.6 14.6 17.2 15.3 16.8 16.1 16.4 17.1 15.6 18 14.5 18H9.5C8.4 18 7.6 17.1 7.2 16.1 6.8 15.3 6.4 14.6 5.9 13.9 5 12.6 4 11.2 4 8.6 4 4.4 7.6 1 12 1Z"
    )
    filament = line("M12 18v-6.5M8.9 11.2A4.7 4.7 0 0 1 15.1 11.2")
    base = path(
        "M8 19H16V19.9A3.2 3.2 0 0 1 12.8 23.1H11.2A3.2 3.2 0 0 1 8 19.9Z"
    ).round(0.5)
    return (bulb - filament.grow(0.0)) | base


@icon("tree")
def _():
    leaf = (circle(18.86, 15.88, 14.3) & circle(6.14, 5.22, 14.3)).round(1)
    return (leaf - line("M4 22C5.6 15.4 9.6 13.6 14.6 9.4", w=1.6)) | line(
        "M4 22C5 17.9 6.6 16.3 8.4 15"
    )


@icon("fried-egg")
def _():
    bowl = path(
        "M2 10.5H22A10 10 0 0 1 17 19.5V20A2 2 0 0 1 15 22H9A2 2 0 0 1 7 20V19.5A10 10 0 0 1 2 10.5Z"
    ).round(0.9)
    food = path("M11.1 9C11.4 6.8 14 5.2 17 5.2S22.6 6.8 22.9 9Z").round(0.8)
    steam = line("M7 2v.3c0 1.2-2 1.6-2 3.4v.6M11 3v.4c0 1.2-2 1.5-2 3.1V8")
    return bowl | (food - rect(0, 9, 24, 3)) | steam


@icon("car")
def _():
    body = rect(2, 10, 20, 10, 2.5)
    cabin = path("M8.3 2H15.7A4 4 0 0 1 19.6 5L20.8 10H3.2L4.4 5A4 4 0 0 1 8.3 2Z")
    window = path("M8.3 4H15.7A2 2 0 0 1 17.6 5.5L18.7 10H5.3L6.4 5.5A2 2 0 0 1 8.3 4Z")
    shell = (cabin - window) | body | line("M2 11h20") | line("M4 19v2M20 19v2")
    lights = line("M5 15h1M10 15h4M18 15h1")
    return shell - lights


@icon("gamepad")
def _():
    pad = path(
        "M4.3 3.1 7.2 2.5A1.6 1.6 0 0 1 8.6 2.9C9 3.3 9.4 3.6 9.9 3.6 11.3 3.5 12.7 3.5 14.1 3.6 14.6 3.6 15 3.3 15.4 2.9A1.6 1.6 0 0 1 16.8 2.5L19.7 3.1C20.4 3.3 20.9 3.9 21 4.6 21.6 4.9 22.1 5.4 22.2 6 22.6 7.3 23 9.7 23 13V17.9A3.1 3.1 0 0 1 17.1 19.3L15.4 15.8C15.1 15.3 14.6 15.1 14.1 15.2 13.5 15.3 12.8 15.5 12 15.5S10.5 15.3 9.9 15.2C9.4 15.1 8.9 15.3 8.6 15.8L6.9 19.3A3.1 3.1 0 0 1 1 17.9V13C1 9.7 1.4 7.3 1.8 6 1.9 5.4 2.4 4.9 3 4.6 3.1 3.9 3.6 3.3 4.3 3.1Z"
    )
    dpad = line("M6 7v4M4 9h4")
    buttons = circle(18.5, 7.5, 1.5) | circle(15.5, 10.5, 1.5)
    return pad - dpad - buttons


def face():
    return disc() - circle(6.5, 11.5, 1.5) - circle(17.5, 11.5, 1.5)


@icon("emoji")
def _():
    return face() - line("M8.5 15a4.4 4.4 0 0 0 7 0")


@icon("neutral")
def _():
    return face() - line("M8 16h8")


@icon("unread")
def _():
    bubble = (circle(12, 12, 10) | path("M3.4 16.8 1.5 21.6V22H13.3V12H3Z")).round(0.6)
    # The bubble's corner is cleared for a turning arrow, its head at the start.
    arrow = line("M15.5 14.8V18H17.4M16.95 16.75A3.6 3.6 0 1 1 16.95 21.85")
    return (bubble - rect(12.6, 12.3, 12, 12, 2)) | arrow


# --- media and voice ----------------------------------------------------------


def ellipse(cx, cy, rx, ry):
    return circle(0, 0, 1).transform(rx, 0, 0, ry, cx, cy)


def struck(shape):
    """Struck through bottom left to top right, as Discord's slashed icons."""
    return cut(shape, line("M2 22 22 2"))


def mic():
    return rect(8, 2, 8, 12, 4) | line("M5 10a7 7 0 0 0 14 0M12 17.5V21M9 21h6")


@icon("microphone")
def _():
    return mic()


@icon("microphone-muted")
def _():
    return struck(mic())


def phones():
    c = (12, 12)
    band = (circle(*c, 11) - circle(*c, 9)) & rect(-1, -1, 26, 14.5)
    # The cup's inner edge runs tangent to its round end (r 2.4 at 8.6,19.3).
    arm = path("M-1 13H5.75A3 3 0 0 1 8.15 14.2L11 18 7 21-1 26Z")
    cup = ((circle(*c, 11) & arm) | circle(8.8, 19.4, 2.5)).round(0.5)
    return band | cup | cup.mirror()


@icon("headphones")
def _():
    return phones()


@icon("headphones-off")
def _():
    return struck(phones())


def speaker():
    return path("M2 7H6L10.6 2H12V22H10.6L6 17H2Z").round(1)


@icon("volume")
def _():
    a = math.radians(66)
    b = math.radians(75)
    inner = f"M{13 + 4 * math.cos(a)} {12 - 4 * math.sin(a)}A4 4 0 0 1 {13 + 4 * math.cos(a)} {12 + 4 * math.sin(a)}"
    outer = f"M{13 + 8 * math.cos(b)} {12 - 8 * math.sin(b)}A8 8 0 0 1 {13 + 8 * math.cos(b)} {12 + 8 * math.sin(b)}"
    return speaker() | line(inner) | line(outer)


@icon("volume-muted")
def _():
    return speaker() | line("M16 9l6 6M22 9l-6 6")


def camera():
    return rect(1, 4, 16, 16, 3) | path("M18.5 9.5 23 7.25V16.75L18.5 14.5Z").round(1)


@icon("video")
def _():
    return camera()


@icon("video-off")
def _():
    return struck(camera())


@icon("screen")
def _():
    return rect(2, 2, 20, 14, 3) | rect(11, 17, 2, 4, 0.5) | line("M9 21h6")


def handset():
    """Discord's handset: a thick arc between two pads, symmetric about y = 24 - x."""
    c = (15.4, 8.6)
    wedge = path("M-2 -2H6.3L12.1 11.9 26 17.7V26H-2Z")
    ear = path("M-2 15.25 15.2 5.4V-3H-2Z")
    mouth = ear.transform(0, -1, -1, 0, 24, 24)
    body = circle(*c, 13.45) & rect(2, 2, 20, 20, 5.4) & wedge
    return (body - (circle(*c, 8.5) - ear - mouth)).round(1)


@icon("phone")
def _():
    return handset()


@icon("hang-up")
def _():
    base = path("M-1 -2H25V12.8H23.2L21.7 17.2 16.1 14.4H7.9L2.3 17.2 0.8 12.8H-1Z")
    hole = circle(12, 14.46, 5.36) & path("M7.55 0H16.45L16.1 20H7.9Z")
    return ((circle(12, 16.66, 11.66) & base) - hole).round(1)


def play():
    return path("M6 1.7 22.6 12 6 22.3Z").round(2)


@icon("play-fill")
def _():
    return play()


@icon("play")
def _():
    return play().hollow()


def pause():
    return rect(5, 4, 5, 16, 1) | rect(14, 4, 5, 16, 1)


@icon("pause-fill")
def _():
    return pause()


@icon("pause")
def _():
    return pause().hollow()


@icon("stop-fill")
def _():
    return rect(3, 3, 18, 18, 3)


@icon("stop")
def _():
    return rect(3, 3, 18, 18, 3).hollow()


@icon("music-note")
def _():
    beam = path("M6 2.4 21 7.2 21 10.2 6 5.2Z").round(1)
    stems = rect(6, 3, 2, 13) | rect(19, 7, 2, 12)
    return beam | stems | ellipse(4.5, 16, 3.5, 3) | ellipse(17.5, 19, 3.5, 3)


@icon("audio")
def _():
    return line("M2 8v8M7 3v18M12 6v12M17 5v14M22 9v6")


@icon("image")
def _():
    sun = circle(7.6, 7.75, 1.9)
    for k in range(8):
        a = math.radians(45 * k)
        sun = sun | circle(7.6 + 1.6 * math.cos(a), 7.75 + 1.6 * math.sin(a), 1.05)
    hills = path("M5 19 8.4 14.7 10 16.5 13.5 11.8 19.1 19Z").round(0.8)
    return rect(2, 2, 20, 20, 3) - sun - hills


@icon("file")
def _():
    body = (rect(2, 2, 20, 20, 4) - rect(14, -6, 14, 16, 5)).round(0.5)
    fold = (path("M16 2 22 8H16Z") & rect(16, -10, 20, 18, 3)).round(0.5)
    return body | fold


@icon("folder")
def _():
    tab = rect(2, 2, 20, 20, 3) & path("M0 0H9.5L12.2 5.5H0Z")
    return (tab | rect(2, 5, 20, 17, 3)).round(1)


@icon("signal")
def _():
    dot = (circle(2, 22, 5) & rect(2, 17, 5, 5, 1)).round(0.8)
    return (
        line("M3 3a18 18 0 0 1 18 18")
        | line("M3 8a13 13 0 0 1 13 13")
        | line("M3 13a8 8 0 0 1 8 8")
        | dot
    )


@icon("eye-off")
def _():
    lid = line(
        "M2.1 12C4 7.5 7.5 4 12 4S20 7.5 21.9 12C20 16.5 16.5 20 12 20S4 16.5 2.1 12Z"
    )
    return struck(lid | circle(12, 12, 4))


@icon("keyboard")
def _():
    keys = rect(7, 15, 10, 2, 0.5)
    for x in (3, 7, 11, 15, 19):
        for y in (7, 11):
            keys = keys | rect(x, y, 2, 2, 0.5)
    return rect(1, 4, 22, 16, 3) - keys


@icon("laptop")
def _():
    return rect(3, 3, 18, 13, 3) | rect(3, 10, 18, 6, 0.5) | line("M2 19h20")


# --- people and the app -------------------------------------------------------


def creases(body, left, right):
    """Cut the two arm creases into a bust's bottom edge."""
    return (
        body
        - line(f"M{left - 0.5} 22.6 {left} 19.6", 1)
        - line(f"M{right + 0.5} 22.6 {right} 19.6", 1)
    )


def person():
    body = path(
        "M2 20.5A9.5 9.5 0 0 1 11.5 11h1A9.5 9.5 0 0 1 22 20.5 1.5 1.5 0 0 1 20.5 22h-17A1.5 1.5 0 0 1 2 20.5Z"
    )
    return circle(12, 6, 4) | creases(body, 5.3, 18.7)


@icon("users")
def _():
    body = path(
        "M2 20.5a7.5 7.5 0 0 1 15 0A1.5 1.5 0 0 1 15.5 22h-12A1.5 1.5 0 0 1 2 20.5Z"
    )
    front = circle(9.5, 9, 3) | creases(body, 5, 14)
    back = circle(14.5, 5, 3) | (circle(14.5, 16.5, 7.5) & rect(0, 0, 22, 18)).round(1)
    return cut(back, front)


@icon("person")
def _():
    return person()


@icon("invite")
def _():
    return badge(person(), PLUS_BADGE)


@icon("leave")
def _():
    door = (
        rect(2, 2, 16, 14, 3)
        | path("M2 5 13 9V20a2.3 2.3 0 0 1-3.4 2L3.6 18A3 3 0 0 1 2 15.5Z").round(0.5)
    ) - path("M4 5 11 7.8V19.8L4.2 16.5Z").round(0.5)
    knob = line("M9 13v2")
    arrow = line("M16 16l5.5 5.5M22 16v6h-6")
    return cut(door | knob, arrow)


@icon("crown")
def _():
    body = path("M0.9 6.6 7.5 10.75 12 4.4 16.5 10.75 23.1 6.6 20.1 16H3.9Z").round(0.8)
    jewel = rect(10.5, 2.4, 3, 3, 0.7).rotate(45, 12, 3.9)
    bowl = (rect(4, 16, 16, 6, 2.4) & rect(0, 18, 24, 6)).round(0.8)
    return body | jewel | bowl


@icon("shield")
def _():
    outer = path(
        "M2 7a1.5 1.5 0 0 1 1.5-1.5h.6A1.5 1.5 0 0 0 5.6 4v-.5A1.5 1.5 0 0 1 7.1 2h9.8a1.5 1.5 0 0 1 1.5 1.5V4a1.5 1.5 0 0 0 1.5 1.5h.6A1.5 1.5 0 0 1 22 7v4c0 3-1 4.7-3 6.7-1.8 1.8-6.6 4.3-7 4.3s-5.2-2.5-7-4.3C3 15.7 2 14 2 11Z"
    )
    inner = path(
        "M9.6 6h4.8c.4 1.6 1.7 2.7 3.4 3.3v1.4c0 2.4-.8 3.5-1.8 4.5-1.1 1-2.8 2-4 2.4-1.2-.4-2.9-1.4-4-2.4-1-1-1.8-2.1-1.8-4.5V9.3C7.9 8.7 9.2 7.6 9.6 6Z"
    )
    return outer.hollow() | inner


@icon("settings")
def _():
    gear = circle(12, 12, 8.1)
    for k in range(8):
        gear = gear | rect(9.7, 0.9, 4.6, 5.2, 1.4).rotate(45 * k)
    return gear - circle(12, 12, 4)


@icon("server")
def _():
    house = path("M8 9.2 16.4 16.3H-.4Z").round(1) | rect(2.5, 14, 11, 8, 2)
    block = path("M3 5.8 8 2 10.8 4.2V11H3Z").round(1)
    tower = path(
        "M12 6.5a2 2 0 0 1 1.3-1.9L20 2.2a1.5 1.5 0 0 1 2 1.4V20a2 2 0 0 1-2 2h-8Z"
    )
    return cut(tower | block, house)


@icon("puzzle")
def _():
    return (rect(4, 3, 18, 18, 3) - circle(13, 4, 3)) | circle(4, 12, 3)


@icon("chart-bar")
def _():
    return line("M4 5h16", 4) | line("M4 12h6", 4) | line("M4 19h12", 4)


@icon("palette")
def _():
    blob = circle(12, 12, 11) - rect(12, 16, 14, 10, 2)
    blob = (blob - (rect(19, 12, 5, 4) - circle(19, 12, 4))).round(1.5)
    holes = (
        circle(12, 4.5, 1.5)
        | circle(17.25, 7.5, 1.5)
        | circle(6.75, 7.5, 1.5)
        | circle(4.5, 13, 1.5)
    )
    return blob - holes


@icon("chat")
def _():
    return circle(12, 12, 10) | path(
        "M12 12 3.55 17.4 3.5 18 1.45 20.35A1 1 0 0 0 2.2 22H12Z"
    )


def archive():
    return rect(2, 2, 20, 20, 3) - (rect(4, 4, 16, 9, 1.5) | circle(12, 14, 3))


@icon("archive")
def _():
    return archive()


@icon("archive-up")
def _():
    return badge(archive(), line("M19 22.5v-7M16 18.5l3-3 3 3"))


@icon("book")
def _():
    outer = rect(2, 2, 16, 20, 3) | rect(5, 5, 17, 17, 1)
    return outer - (rect(18, 4, 2, 16) | rect(4, 17, 16, 3, 1.5))


@icon("grid")
def _():
    g = rect(2, 2, 9, 9, 2)
    return (
        g
        | g.transform(1, 0, 0, 1, 11, 0)
        | g.transform(1, 0, 0, 1, 0, 11)
        | g.transform(1, 0, 0, 1, 11, 11)
    )


@icon("text")
def _():
    return line("M3 5h18M3 10h18M3 15h18M3 20h11")


@icon("calendar")
def _():
    head = (rect(2, 3, 20, 10, 3) & rect(0, 0, 24, 7)).round(1)
    body = (rect(2, 4, 20, 18, 3) & rect(0, 9, 24, 15)).round(1) - rect(
        5, 12, 4, 4, 0.5
    )
    return head | body | line("M7 2v1.5M17 2v1.5")


# --- channels -----------------------------------------------------------------


def lock_badge(y=4):
    """The padlock badge: an 8 by 6 body with its shackle above, at the right."""
    return rect(15, y, 8, 6, 1) | line(
        f"M17.25 {y + 0.5}V{y - 0.5}a1.75 1.75 0 0 1 3.5 0V{y + 0.5}", w=1.5
    )


def locked(shape, y=4):
    """`shape` with the padlock in its top right corner (y=4) or bottom right
    (y=18), cleared by the gap around the padlock's whole box."""
    return (shape - rect(15, y - 3, 8, 9, 1).grow(GAP)) | lock_badge(y)


def megaphone():
    cone = path("M6 8.4 17 4.4V19.8L6 16.2Z")
    bar = rect(16.8, 2, 5.2, 20, 2)
    foot = path(
        "M6 15 13.6 17.8 12.4 20.2a3.5 3.5 0 0 1-3.9 1.7l-.4-.1A2.8 2.8 0 0 1 6 19.1Z"
    )
    hole = path("M8 16.7 11 17.8 10.4 19a1.2 1.2 0 0 1-1.4.6l-.3-.1A1 1 0 0 1 8 18.5Z")
    tab = rect(2, 8, 4, 8, 2) & rect(0, 0, 5, 24)
    return ((cone | bar | foot).round(1.2) - hole) | tab


def forum():
    big = ellipse(11, 10, 9, 8) | path("M4 13 1.6 16.3a1 1 0 0 0 .8 1.7H8V13Z")
    small = circle(15.7, 17.5, 5.5) | path(
        "M15.7 17.5 20.6 17.8 22.2 21.5a1 1 0 0 1-.9 1.5H15.7Z"
    )
    return cut(big, small)


def taper(x1, y1, w1, x2, y2, w2):
    """A bar with round ends, w1 thick at one end and w2 at the other."""
    dx, dy = x2 - x1, y2 - y1
    n = math.hypot(dx, dy)
    px, py = -dy / n, dx / n
    a, b = w1 / 2, w2 / 2
    quad = path(
        f"M{x1 + px * a} {y1 + py * a}L{x2 + px * b} {y2 + py * b}"
        f"L{x2 - px * b} {y2 - py * b}L{x1 - px * a} {y1 - py * a}Z"
    )
    return quad | circle(x1, y1, a) | circle(x2, y2, b)


def thread():
    return (
        taper(12.9, 2.1, 1.8, 21.9, 10.4, 3)
        | line("M9.2 7.8 17.6 13.4")
        | line("M6.4 10.6 15 16.4")
        | taper(2.1, 13.6, 3, 11.1, 21.9, 1.8)
    )


def stage():
    body = circle(12, 22, 6) & circle(12, 12, 11)
    return (
        line("M3.8 17.7A10 10 0 1 1 20.2 17.7")
        | line("M6.8 15A6 6 0 1 1 17.2 15")
        | circle(12, 12, 3)
        | body.round(1)
    )


@icon("megaphone")
def _():
    return megaphone()


@icon("forum")
def _():
    return forum()


@icon("thread")
def _():
    return thread()


@icon("stage")
def _():
    return stage()


@icon("hash-lock")
def _():
    return locked(ICONS["hash"]())


@icon("volume-lock")
def _():
    return locked(ICONS["volume"]())


@icon("megaphone-lock")
def _():
    return locked(megaphone())


@icon("forum-lock")
def _():
    return locked(forum())


@icon("thread-lock")
def _():
    return locked(thread(), y=18)


@icon("stage-lock")
def _():
    return locked(stage(), y=18)


# --- output ----------------------------------------------------------------


def d_of(shape):
    p = pathops.Path()
    p.addPath(shape.p)
    p.convertConicsToQuads()
    p.simplify(fix_winding=True)

    def f(v):
        s = f"{round(v, 2):.2f}".rstrip("0").rstrip(".")
        return "0" if s in ("-0", "") else s

    letter = {
        pathops.PathVerb.MOVE: "M",
        pathops.PathVerb.LINE: "L",
        pathops.PathVerb.QUAD: "Q",
        pathops.PathVerb.CUBIC: "C",
        pathops.PathVerb.CLOSE: "Z",
    }
    return "".join(
        letter[verb] + " ".join(f"{f(x)} {f(y)}" for x, y in pts) for verb, pts in p
    )


def svg(shape):
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor">'
        f'<path fill-rule="evenodd" d="{d_of(shape)}"/></svg>\n'
    )


if __name__ == "__main__":
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)
    for name, draw in ICONS.items():
        with open(os.path.join(out, f"{name}.svg"), "w") as fh:
            fh.write(svg(draw()))
    print(len(ICONS), "icons")
