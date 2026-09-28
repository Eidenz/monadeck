"""Builds the controller drawings the binding editor shows: SteamVR-style white
line art on transparent PNGs, rasterised from the desktop editor's SVGs
(desktop/static/bindings) at the size the overlay draws them, so the lines
land on whole pixels instead of being resampled.

Each drawing is of a RIGHT controller; the editor mirrors it for the left.
The Touch drawing has its B / A letters and the Oculus logo stripped (the
last paths of the file): mirrored they'd read backwards, so the editor paints
X / Y and A / B itself. The desktop editor draws the SVGs directly; its
letterless Touch one (`oculus_touch_right_blank.svg`) is written here too.

    python3 build_art.py      # needs inkscape; rewrites the PNGs next to it
"""
import os
import re
import subprocess
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
SRC = os.path.join(HERE, "..", "..", "..", "..", "desktop", "static", "bindings")

# (svg, png, export flag, pixels): the editor fits each into a 214x300 pt box
# at 1.5 px per point.
ART = [
    ("indexcontroller_right.svg", "index.png", "--export-height", 450),
    ("oculus_touch_right.svg", "touch.png", "--export-width", 321),
    ("vive_wand.svg", "vive.png", "--export-height", 450),
]


def strip_touch_letters(svg):
    # Path 0 is the controller; 3..6 are "B", "A" and the logo (1, 2 are empty).
    paths = re.findall(r"<path[^>]*?/>", svg, re.S)
    for p in paths[3:]:
        svg = svg.replace(p, "")
    return svg


def main():
    with open(os.path.join(SRC, "oculus_touch_right.svg")) as f:
        blank = strip_touch_letters(f.read())
    with open(os.path.join(SRC, "oculus_touch_right_blank.svg"), "w") as f:
        f.write(blank)
    print("wrote oculus_touch_right_blank.svg")
    for src, out, flag, px in ART:
        with open(os.path.join(SRC, src)) as f:
            svg = f.read()
        if src.startswith("oculus_touch"):
            svg = strip_touch_letters(svg)
        with tempfile.NamedTemporaryFile("w", suffix=".svg", delete=False) as tmp:
            tmp.write(svg)
        subprocess.run(
            ["inkscape", tmp.name, "--export-type=png", f"--export-filename={os.path.join(HERE, out)}", f"{flag}={px}"],
            check=True,
            stderr=subprocess.DEVNULL,
        )
        os.unlink(tmp.name)
        print("wrote", out)


if __name__ == "__main__":
    main()
