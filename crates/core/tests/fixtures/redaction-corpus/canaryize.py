#!/usr/bin/env python3
"""Shape-derive a key fixture from real tool output.

Follows the standing rule stated in
`crates/core/tests/fixtures/pgp_armor_shapes.md`: every dimension the
detector actually tests -- banner text, presence and position of armor
headers, blank lines, per-line widths, the CRC line, the END banner, and
the trailing base64 padding -- is the real thing; only the key bytes are
replaced, because committing a private key to a repository is not made
acceptable by the key being disposable.

Payload bytes come from a fixed-seed PRNG over the base64 alphabet, with
`TLCANARY` planted at the head of the payload so a leak has a marker.
Deterministic: the same input structure always yields the same fixture.

Usage: canaryize.py <real-key-file> <seed-int>
"""
import random, re, sys

B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
CANARY = "TLCANARY"


def canaryize(text: str, seed: int) -> str:
    rng = random.Random(seed)
    lines = text.rstrip("\n").split("\n")
    begin, end, body = lines[0], lines[-1], lines[1:-1]

    # Total payload characters, so the substitute is the same length overall.
    payload_idx = []
    for i, l in enumerate(body):
        if l == "" or l.startswith("=") or re.match(r"^[A-Za-z][A-Za-z0-9-]*: ", l):
            continue                      # blank line, CRC line, armor header
        payload_idx.append(i)

    total = sum(len(body[i]) for i in payload_idx)
    # Trailing '=' padding on the final payload line is structural: preserve it.
    pad = 0
    if payload_idx:
        last = body[payload_idx[-1]]
        pad = len(last) - len(last.rstrip("="))

    stream = CANARY + "".join(rng.choice(B64) for _ in range(total - len(CANARY) - pad))
    stream += "=" * pad
    # A pure-hex run reads as a digest and is excluded by the detector
    # (A209-C11), so guarantee a non-hex character early in every line.
    out, pos = list(body), 0
    for i in payload_idx:
        w = len(body[i])
        chunk = list(stream[pos:pos + w]); pos += w
        if w >= 8 and all(c in "0123456789abcdefABCDEF" for c in chunk):
            chunk[3] = "+"
        out[i] = "".join(chunk)
    return "\n".join([begin] + out + [end]) + "\n"


if __name__ == "__main__":
    sys.stdout.write(canaryize(open(sys.argv[1]).read(), int(sys.argv[2])))
