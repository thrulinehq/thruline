"""Pure-Python NaCl crypto_secretbox (XSalsa20-Poly1305).

No third-party dependency: PyNaCl is NOT installed on this machine, and
`cryptography` 49.0.0 exposes no Salsa20/XSalsa20 primitive. Correctness is
established against the golden vectors in
  vendor/gossl/encrypted/encrypted_test.go
and the cosign key fixtures with the known password "hello" (see verify_all.py).

Byte layout mirrors golang.org/x/crypto/nacl/secretbox.Seal, which returns
    tag(16 bytes) || ciphertext(len(plaintext) bytes)
see source/xcrypto-nacl-secretbox.go.
"""

import struct

MASK = 0xFFFFFFFF
SIGMA = b"expand 32-byte k"
TAG_SIZE = 16
KEY_SIZE = 32
NONCE_SIZE = 24


def _rotl32(x, n):
    x &= MASK
    return ((x << n) | (x >> (32 - n))) & MASK


def _double_rounds(x, rounds=20):
    for _ in range(rounds // 2):
        # column round
        x[4] ^= _rotl32(x[0] + x[12], 7)
        x[8] ^= _rotl32(x[4] + x[0], 9)
        x[12] ^= _rotl32(x[8] + x[4], 13)
        x[0] ^= _rotl32(x[12] + x[8], 18)
        x[9] ^= _rotl32(x[5] + x[1], 7)
        x[13] ^= _rotl32(x[9] + x[5], 9)
        x[1] ^= _rotl32(x[13] + x[9], 13)
        x[5] ^= _rotl32(x[1] + x[13], 18)
        x[14] ^= _rotl32(x[10] + x[6], 7)
        x[2] ^= _rotl32(x[14] + x[10], 9)
        x[6] ^= _rotl32(x[2] + x[14], 13)
        x[10] ^= _rotl32(x[6] + x[2], 18)
        x[3] ^= _rotl32(x[15] + x[11], 7)
        x[7] ^= _rotl32(x[3] + x[15], 9)
        x[11] ^= _rotl32(x[7] + x[3], 13)
        x[15] ^= _rotl32(x[11] + x[7], 18)
        # row round
        x[1] ^= _rotl32(x[0] + x[3], 7)
        x[2] ^= _rotl32(x[1] + x[0], 9)
        x[3] ^= _rotl32(x[2] + x[1], 13)
        x[0] ^= _rotl32(x[3] + x[2], 18)
        x[6] ^= _rotl32(x[5] + x[4], 7)
        x[7] ^= _rotl32(x[6] + x[5], 9)
        x[4] ^= _rotl32(x[7] + x[6], 13)
        x[5] ^= _rotl32(x[4] + x[7], 18)
        x[11] ^= _rotl32(x[10] + x[9], 7)
        x[8] ^= _rotl32(x[11] + x[10], 9)
        x[9] ^= _rotl32(x[8] + x[11], 13)
        x[10] ^= _rotl32(x[9] + x[8], 18)
        x[12] ^= _rotl32(x[15] + x[14], 7)
        x[13] ^= _rotl32(x[12] + x[15], 9)
        x[14] ^= _rotl32(x[13] + x[12], 13)
        x[15] ^= _rotl32(x[14] + x[13], 18)
    return x


def _state(key, inp16, sigma=SIGMA):
    """Salsa20 initial state: c0 k[0:16] c1 in[0:16] c2 k[16:32] c3."""
    c = struct.unpack("<4I", sigma)
    k = struct.unpack("<8I", key)
    n = struct.unpack("<4I", inp16)
    return [
        c[0], k[0], k[1], k[2],
        k[3], c[1], n[0], n[1],
        n[2], n[3], c[2], k[4],
        k[5], k[6], k[7], c[3],
    ]


def hsalsa20(key, inp16):
    """HSalsa20: 20 rounds, NO feed-forward; take words 0,5,10,15,6,7,8,9."""
    x = _double_rounds(_state(key, inp16))
    return struct.pack("<8I", x[0], x[5], x[10], x[15], x[6], x[7], x[8], x[9])


def _salsa20_block(key, counter16):
    inp = _state(key, counter16)
    x = _double_rounds(list(inp))
    return struct.pack("<16I", *[(x[i] + inp[i]) & MASK for i in range(16)])


def _xor_keystream(key, counter16, length, skip_blocks=0):
    """Generate `length` keystream bytes starting at block `skip_blocks`.

    counter16 is the 16-byte (nonce8 || counter8) input block; the 64-bit
    little-endian block counter lives at bytes [8:16].
    """
    out = bytearray()
    ctr = bytearray(counter16)
    block = skip_blocks
    while len(out) < length:
        ctr[8:16] = struct.pack("<Q", block)
        out += _salsa20_block(key, bytes(ctr))
        block += 1
    return bytes(out[:length])


def _clamp_r(r):
    return r & 0x0FFFFFFC0FFFFFFC0FFFFFFC0FFFFFFF


def poly1305(msg, key32):
    r = _clamp_r(int.from_bytes(key32[:16], "little"))
    s = int.from_bytes(key32[16:], "little")
    p = (1 << 130) - 5
    acc = 0
    for i in range(0, len(msg), 16):
        chunk = msg[i:i + 16]
        n = int.from_bytes(chunk + b"\x01", "little")
        acc = ((acc + n) * r) % p
    acc = (acc + s) & ((1 << 128) - 1)
    return acc.to_bytes(16, "little")


def _setup(key, nonce):
    """XSalsa20 setup: subkey = HSalsa20(key, nonce[:16]); counter = nonce[16:24]||0."""
    subkey = hsalsa20(key, nonce[:16])
    counter16 = nonce[16:24] + b"\x00" * 8
    return subkey, counter16


def secretbox_seal(plaintext, nonce, key):
    """NaCl crypto_secretbox. Returns tag(16) || ciphertext (Go's Seal layout)."""
    assert len(key) == KEY_SIZE, "incorrect key size"
    assert len(nonce) == NONCE_SIZE, "incorrect nonce size"
    subkey, counter16 = _setup(key, nonce)
    first_block = _xor_keystream(subkey, counter16, 64, skip_blocks=0)
    poly_key = first_block[:32]
    stream = first_block[32:] + _xor_keystream(
        subkey, counter16, max(0, len(plaintext) - 32), skip_blocks=1
    )
    ct = bytes(a ^ b for a, b in zip(plaintext, stream))
    return poly1305(ct, poly_key) + ct


def secretbox_open(box, nonce, key):
    """Inverse of secretbox_seal. Raises ValueError on authentication failure."""
    assert len(key) == KEY_SIZE, "incorrect key size"
    if len(nonce) != NONCE_SIZE:
        raise ValueError("encrypted: incorrect nonce size")
    if len(box) < TAG_SIZE:
        raise ValueError("encrypted: decryption failed")
    tag, ct = box[:TAG_SIZE], box[TAG_SIZE:]
    subkey, counter16 = _setup(key, nonce)
    first_block = _xor_keystream(subkey, counter16, 64, skip_blocks=0)
    poly_key = first_block[:32]
    expected = poly1305(ct, poly_key)
    # constant-time-ish compare
    if len(expected) != len(tag) or sum(a ^ b for a, b in zip(expected, tag)) != 0:
        raise ValueError("encrypted: decryption failed")
    stream = first_block[32:] + _xor_keystream(
        subkey, counter16, max(0, len(ct) - 32), skip_blocks=1
    )
    return bytes(a ^ b for a, b in zip(ct, stream))
