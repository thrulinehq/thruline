#!/usr/bin/env python3
"""pixdiff.py — how far a product screen is from its HQ Next artboard.

    python3 scripts/hq/pixdiff.py <reference.png> <product.png> [<diff.png>]

Prints the share of pixels that differ by more than a small tolerance (per
channel > 24 of 255, which absorbs antialiasing and font-hinting noise but not
a moved box or a changed colour), the bounding box of the difference, and,
with a third argument, writes an image: the product dimmed, every differing
pixel painted orange — so the eye goes straight to what does not match.
"""
import sys

import numpy as np
from PIL import Image


def main():
    ref_p, out_p = sys.argv[1], sys.argv[2]
    diff_p = sys.argv[3] if len(sys.argv) > 3 else None
    a = Image.open(ref_p).convert("RGB")
    b = Image.open(out_p).convert("RGB")
    w, h = min(a.width, b.width), min(a.height, b.height)
    size_note = "" if a.size == b.size else f"  (sizes differ: reference {a.size}, product {b.size}; compared {w}x{h})"
    A = np.asarray(a.crop((0, 0, w, h)), dtype=np.int16)
    B = np.asarray(b.crop((0, 0, w, h)), dtype=np.int16)
    mask = (np.abs(A - B) > 24).any(axis=2)
    share = mask.mean() * 100
    if mask.any():
        ys, xs = np.where(mask)
        box = f"x {xs.min()}–{xs.max()}, y {ys.min()}–{ys.max()}"
    else:
        box = "none"
    print(f"{share:6.2f}% differ  box: {box}{size_note}")
    if diff_p:
        img = (B * 0.35).astype(np.uint8)
        img[mask] = [255, 122, 41]
        Image.fromarray(img).save(diff_p)


if __name__ == "__main__":
    main()
