#!/usr/bin/env python3
"""side-by-side.py — one sign-off sheet per screen: the founder's artboard and
the same screen on the real record, side by side, with the design-mode pixdiff
and one line per intended difference.

    python3 scripts/hq/side-by-side.py <out.png> <artboard.png> <live.png> \
        --title "Inbox" --pixdiff "0.00% differ  box: none" \
        --note "real data: 144 threads of the ThruLine project" --note "…"

Both images are shown at their own 1512-wide size (cropped to 920 tall unless
--full), on the canvas's black ground, labelled ARTBOARD and REAL RECORD. The
notes are the only words added; nothing is drawn over either screen.
"""
import argparse

from PIL import Image, ImageDraw, ImageFont

GROUND = (0, 0, 0)
T1 = (237, 237, 239)
T2 = (161, 161, 170)
ORANGE = (255, 122, 41)
LINE = (42, 42, 48)


def font(size, mono=False):
    candidates = (
        ["/System/Library/Fonts/SFNSMono.ttf", "/System/Library/Fonts/Menlo.ttc"]
        if mono
        else ["/System/Library/Fonts/SFNS.ttf", "/System/Library/Fonts/Helvetica.ttc"]
    )
    for path in candidates:
        try:
            return ImageFont.truetype(path, size)
        except OSError:
            continue
    return ImageFont.load_default()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("out")
    ap.add_argument("artboard")
    ap.add_argument("live")
    ap.add_argument("--title", required=True)
    ap.add_argument("--pixdiff", required=True)
    ap.add_argument("--note", action="append", default=[])
    ap.add_argument("--full", action="store_true", help="keep the full height of tall pages")
    a = ap.parse_args()

    art = Image.open(a.artboard).convert("RGB")
    live = Image.open(a.live).convert("RGB")
    h = max(art.height, live.height) if a.full else 920
    art = art.crop((0, 0, 1512, min(h, art.height)))
    live = live.crop((0, 0, 1512, min(h, live.height)))

    pad, gap, head = 40, 40, 150
    notes_h = 34 * max(1, len(a.note)) + 30
    W = pad * 2 + 1512 * 2 + gap
    H = head + h + notes_h + pad
    sheet = Image.new("RGB", (W, H), GROUND)
    d = ImageDraw.Draw(sheet)

    d.text((pad, 28), a.title, font=font(40), fill=T1)
    d.text((pad, 84), "Design-mode pixdiff vs the artboard: " + a.pixdiff, font=font(22, mono=True), fill=T2)
    for x, label, colour in ((pad, "ARTBOARD (the canvas)", T2), (pad + 1512 + gap, "REAL RECORD (this build)", ORANGE)):
        d.text((x, head - 34), label, font=font(20), fill=colour)
    sheet.paste(art, (pad, head))
    sheet.paste(live, (pad + 1512 + gap, head))
    d.rectangle([pad - 1, head - 1, pad + 1512, head + art.height], outline=LINE)
    d.rectangle([pad + 1512 + gap - 1, head - 1, pad + 1512 * 2 + gap, head + live.height], outline=LINE)

    y = head + h + 24
    d.text((pad, y), "Intended differences:" if a.note else "Intended differences: none", font=font(22), fill=T1)
    for i, n in enumerate(a.note):
        d.text((pad + 24, y + 34 * (i + 1)), "· " + n, font=font(20), fill=T2)
    sheet.save(a.out, optimize=True)


if __name__ == "__main__":
    main()
