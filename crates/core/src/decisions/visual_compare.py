"""Union-canvas, masked PNG pixel census for Decisions C1.

The Rust caller fixes every path, rectangle and threshold. This tool prints one
JSON object and never decides a verdict. More than 24/255 in any RGB channel is
one differing pixel, matching the HQ Next reference comparison convention.
"""

import json
import hashlib
import sys

import numpy as np
from PIL import Image


def main():
    reference_path, screenshot_path, diff_path, rectangles_json = sys.argv[1:5]
    rectangles = json.loads(rectangles_json)
    reference = Image.open(reference_path).convert("RGB")
    screenshot = Image.open(screenshot_path).convert("RGB")
    reference_width, reference_height = reference.size
    capture_width, capture_height = screenshot.size
    width = max(reference_width, capture_width)
    height = max(reference_height, capture_height)
    a = np.zeros((height, width, 3), dtype=np.int16)
    b = np.zeros((height, width, 3), dtype=np.int16)
    a[:reference_height, :reference_width] = np.asarray(reference, dtype=np.int16)
    b[:capture_height, :capture_width] = np.asarray(screenshot, dtype=np.int16)
    reference_present = np.zeros((height, width), dtype=bool)
    capture_present = np.zeros((height, width), dtype=bool)
    reference_present[:reference_height, :reference_width] = True
    capture_present[:capture_height, :capture_width] = True
    changed = (reference_present ^ capture_present) | (
        reference_present & capture_present & (np.abs(a - b) > 24).any(axis=2)
    )
    eligible = np.ones((height, width), dtype=bool)
    for rectangle in rectangles:
        x, y = rectangle["x"], rectangle["y"]
        w, h = rectangle["width"], rectangle["height"]
        if w <= 0 or h <= 0 or x < 0 or y < 0 or x + w > width or y + h > height:
            print(json.dumps({"error": "invalid_mask"}))
            return
        eligible[y:y + h, x:x + w] = False
    total = int(eligible.sum())
    if total == 0:
        print(json.dumps({"error": "all_pixels_masked"}))
        return
    changed &= eligible
    differing = int(changed.sum())
    fingerprint = hashlib.sha256()
    fingerprint.update(f"{reference_width}x{reference_height}:{capture_width}x{capture_height}".encode())
    # The receipt identifies the compared result, not raw capture pixels that
    # were already classified as different (or below the accepted threshold).
    # This is also the information rendered in the diff artifact below.
    fingerprint.update(eligible.tobytes())
    fingerprint.update(changed.tobytes())
    comparison_sha256 = fingerprint.hexdigest()
    if diff_path:
        # A pinned reference supplies stable context. Live capture text inside
        # orange differing pixels would otherwise change the PNG bytes while
        # the comparison result stays exactly the same.
        image = (a.astype(np.float32) * 0.35).astype(np.uint8)
        image[~reference_present] = [0, 0, 0]
        image[~eligible] = [0, 0, 0]
        image[changed] = [255, 122, 41]
        Image.fromarray(image).save(diff_path)
    print(json.dumps({"different_pixels": differing, "checked_pixels": total,
                      "difference_pct": differing * 100.0 / total,
                      "reference_width": reference_width,
                      "reference_height": reference_height,
                      "capture_width": capture_width,
                      "capture_height": capture_height,
                      "compared_width": width, "compared_height": height,
                      "dimensions_differ": reference.size != screenshot.size,
                      "comparison_sha256": comparison_sha256}))


if __name__ == "__main__":
    main()
