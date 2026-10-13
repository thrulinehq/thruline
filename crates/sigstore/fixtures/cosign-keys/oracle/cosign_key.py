"""Read and write cosign-format encrypted private keys (cosign.key / cosign.pub).

Derived byte-for-byte from source, not from memory. Every constant below
carries a source citation; the Go files quoted are copied under ./source/.

Wire format
-----------
cosign.key  = PEM, type "ENCRYPTED SIGSTORE PRIVATE KEY" (>= cosign v2.0.1)
                 or "ENCRYPTED COSIGN PRIVATE KEY" (legacy, still accepted)
              no PEM headers/attributes; body is base64 std, wrapped at 64 cols
PEM body    = UTF-8 JSON, compact (Go encoding/json, struct field order):
  {"kdf":{"name":"scrypt","params":{"N":<int>,"r":8,"p":1},"salt":"<b64 32B>"},
   "cipher":{"name":"nacl/secretbox","nonce":"<b64 24B>"},
   "ciphertext":"<b64 (16+len) B>"}
ciphertext  = NaCl secretbox output = Poly1305 tag(16) || XSalsa20 ct
plaintext   = PKCS#8 DER of the private key (unencrypted PrivateKeyInfo)
cosign.pub  = PEM, type "PUBLIC KEY", PKIX/SPKI DER

Usage:
    python3 cosign_key.py generate --out /tmp/cosign --password hunter2
    python3 cosign_key.py read     --key /tmp/cosign.key --password hunter2
    python3 cosign_key.py sign     --key /tmp/cosign.key --password hunter2 --in FILE
    python3 cosign_key.py verify   --pub /tmp/cosign.pub --in FILE --sig SIGFILE
"""

import argparse
import base64
import hashlib
import json
import os
import sys

from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import ec

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from xsalsa20poly1305 import secretbox_open, secretbox_seal  # noqa: E402

# ---- constants, each traced to source ---------------------------------------
# source/cosign-v3-pkg-cosign-keys.go:42-43
SIGSTORE_PEM_TYPE = "ENCRYPTED SIGSTORE PRIVATE KEY"
COSIGN_PEM_TYPE = "ENCRYPTED COSIGN PRIVATE KEY"
# source/sigstore-v1.10.8-cryptoutils-publickey.go:37
PUBLIC_KEY_PEM_TYPE = "PUBLIC KEY"

# source/gossl-v0.11.0-encrypted.go:19,22,23
SALT_SIZE = 32
BOX_KEY_SIZE = 32
BOX_NONCE_SIZE = 24
# source/gossl-v0.11.0-encrypted.go:69-70
NAME_SCRYPT = "scrypt"
NAME_SECRETBOX = "nacl/secretbox"

# source/gossl-v0.11.0-encrypted.go:42-61. The ONLY three accepted by
# CheckParams() (line 128-138); anything else is refused at decrypt time.
KDF_LEGACY = {"N": 32768, "r": 8, "p": 1}    # go-tuf era; cosign <= v2.1.x
KDF_STANDARD = {"N": 65536, "r": 8, "p": 1}  # default since cosign v2.2.0
KDF_OWASP = {"N": 131072, "r": 8, "p": 1}
ACCEPTED_KDF_PARAMS = (KDF_LEGACY, KDF_STANDARD, KDF_OWASP)
# encrypted.Encrypt() -> EncryptWithCustomKDFParameters(..., Standard)
# source/gossl-v0.11.0-encrypted.go:205-207, called from keys.go:189
DEFAULT_KDF_PARAMS = KDF_STANDARD

# 128 * N * r bytes of scratch; hashlib.scrypt's OpenSSL default maxmem (32 MiB)
# is too small for N=65536 (64 MiB) and N=131072 (128 MiB).
SCRYPT_MAXMEM = 512 * 1024 * 1024


class CosignKeyError(Exception):
    pass


# ---- PEM (Go encoding/pem semantics: 64 base64 chars per line) ---------------

def pem_encode(pem_type, der):
    b64 = base64.b64encode(der).decode("ascii")
    lines = [b64[i:i + 64] for i in range(0, len(b64), 64)]
    return (
        f"-----BEGIN {pem_type}-----\n"
        + "".join(line + "\n" for line in lines)
        + f"-----END {pem_type}-----\n"
    ).encode("ascii")


def pem_decode(data):
    """Return (type, der) for the FIRST PEM block, matching Go's pem.Decode."""
    if isinstance(data, bytes):
        data = data.decode("utf-8", "replace")
    lines = data.splitlines()
    start = None
    for i, line in enumerate(lines):
        if line.startswith("-----BEGIN ") and line.rstrip().endswith("-----"):
            start = i
            break
    if start is None:
        raise CosignKeyError("invalid pem block")
    pem_type = lines[start].strip()[len("-----BEGIN "):-len("-----")]
    body = []
    for line in lines[start + 1:]:
        if line.startswith("-----END "):
            return pem_type, base64.b64decode("".join(body))
        if ":" in line and not body:
            continue  # PEM header line (RFC 1421); cosign emits none
        body.append(line.strip())
    raise CosignKeyError("invalid pem block")


# ---- the encrypted envelope -------------------------------------------------

def _scrypt_key(passphrase, salt, params):
    # gossl encrypted.go:121-123 -> scrypt.Key(pass, salt, N, r, p, 32)
    return hashlib.scrypt(
        passphrase, salt=salt, n=params["N"], r=params["r"], p=params["p"],
        dklen=BOX_KEY_SIZE, maxmem=SCRYPT_MAXMEM,
    )


def encrypted_encrypt(plaintext, passphrase, kdf_params=None, salt=None, nonce=None):
    """encrypted.Encrypt(): returns the compact JSON envelope as bytes."""
    params = dict(kdf_params or DEFAULT_KDF_PARAMS)
    salt = salt if salt is not None else os.urandom(SALT_SIZE)
    nonce = nonce if nonce is not None else os.urandom(BOX_NONCE_SIZE)
    if len(salt) != SALT_SIZE:
        raise CosignKeyError(f"salt must be {SALT_SIZE} bytes")
    if len(nonce) != BOX_NONCE_SIZE:
        raise CosignKeyError(f"nonce must be {BOX_NONCE_SIZE} bytes")
    key = _scrypt_key(passphrase, salt, params)
    box = secretbox_seal(plaintext, nonce, key)
    envelope = {
        "kdf": {
            "name": NAME_SCRYPT,
            "params": {"N": params["N"], "r": params["r"], "p": params["p"]},
            "salt": base64.b64encode(salt).decode("ascii"),
        },
        "cipher": {
            "name": NAME_SECRETBOX,
            "nonce": base64.b64encode(nonce).decode("ascii"),
        },
        "ciphertext": base64.b64encode(box).decode("ascii"),
    }
    # Go's json.Marshal is compact and emits struct fields in declaration order,
    # which is exactly the insertion order above (encrypted.go:73-77, 115-119, 151-153).
    return json.dumps(envelope, separators=(",", ":")).encode("utf-8")


def encrypted_decrypt(envelope_bytes, passphrase):
    """encrypted.Decrypt(), including the CheckParams() anti-DoS check."""
    try:
        d = json.loads(envelope_bytes)
    except Exception as e:
        raise CosignKeyError(f"invalid envelope json: {e}") from e
    if d.get("kdf", {}).get("name") != NAME_SCRYPT:
        raise CosignKeyError(
            'encrypted: unknown kdf name "%s"' % d.get("kdf", {}).get("name"))
    if d.get("cipher", {}).get("name") != NAME_SECRETBOX:
        raise CosignKeyError(
            'encrypted: unknown cipher name "%s"' % d.get("cipher", {}).get("name"))
    params = d["kdf"]["params"]
    norm = {"N": params.get("N"), "r": params.get("r"), "p": params.get("p")}
    if norm not in ACCEPTED_KDF_PARAMS:
        raise CosignKeyError("unsupported scrypt parameters")
    salt = base64.b64decode(d["kdf"]["salt"])
    nonce = base64.b64decode(d["cipher"]["nonce"])
    box = base64.b64decode(d["ciphertext"])
    key = _scrypt_key(passphrase, salt, norm)
    try:
        return secretbox_open(box, nonce, key)
    except ValueError as e:
        # gossl encrypted.go:198 -> errors.New("encrypted: decryption failed")
        raise CosignKeyError(str(e)) from None


# ---- cosign.key / cosign.pub ------------------------------------------------

def write_key_pair(private_key, passphrase, pem_type=SIGSTORE_PEM_TYPE,
                   kdf_params=None):
    """Return (cosign.key bytes, cosign.pub bytes).

    keys.go:181-215 -> x509.MarshalPKCS8PrivateKey, encrypted.Encrypt,
    pem.EncodeToMemory, cryptoutils.MarshalPublicKeyToPEM.
    """
    pkcs8_der = private_key.private_bytes(
        encoding=serialization.Encoding.DER,
        format=serialization.PrivateFormat.PKCS8,
        encryption_algorithm=serialization.NoEncryption(),
    )
    envelope = encrypted_encrypt(pkcs8_der, passphrase, kdf_params=kdf_params)
    priv_pem = pem_encode(pem_type, envelope)
    pub_pem = private_key.public_key().public_bytes(
        encoding=serialization.Encoding.PEM,
        format=serialization.PublicFormat.SubjectPublicKeyInfo,
    )
    return priv_pem, pub_pem


def load_private_key(key_pem, passphrase):
    """cosign.LoadPrivateKey(): PEM -> decrypt -> ParsePKCS8PrivateKey."""
    pem_type, envelope = pem_decode(key_pem)
    if pem_type not in (COSIGN_PEM_TYPE, SIGSTORE_PEM_TYPE):
        raise CosignKeyError(f"unsupported pem type: {pem_type}")
    der = encrypted_decrypt(envelope, passphrase)
    # keys.go:225 requires PKCS#8. A SEC1 ("EC PRIVATE KEY") payload is rejected;
    # see keys_test.go pemcosigneckey, which expects exactly that failure.
    return serialization.load_der_private_key(der, password=None), der, pem_type


def generate(passphrase, curve=ec.SECP256R1(), pem_type=SIGSTORE_PEM_TYPE,
             kdf_params=None):
    priv = ec.generate_private_key(curve)
    return priv, *write_key_pair(priv, passphrase, pem_type, kdf_params)


# ---- signing (sigstore ECDSASigner: ASN.1 DER over SHA-256) -----------------
# source/sigstore-v1.10.8-signature-ecdsa.go:94 -> ecdsa.SignASN1
# Hash is SHA-256 for P-256 (signerverifier.go:51).

def sign(private_key, message):
    return private_key.sign(message, ec.ECDSA(hashes.SHA256()))


def verify(public_key, signature_der, message):
    public_key.verify(signature_der, message, ec.ECDSA(hashes.SHA256()))


# ---- CLI --------------------------------------------------------------------

def _pw(args):
    if args.password is not None:
        return args.password.encode()
    return os.environ.get("COSIGN_PASSWORD", "").encode()


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="cmd", required=True)

    g = sub.add_parser("generate", help="write <out>.key and <out>.pub")
    g.add_argument("--out", default="cosign")
    g.add_argument("--password")
    g.add_argument("--pem-type", default=SIGSTORE_PEM_TYPE,
                   choices=[SIGSTORE_PEM_TYPE, COSIGN_PEM_TYPE])
    g.add_argument("--kdf", default="standard",
                   choices=["legacy", "standard", "owasp"])

    r = sub.add_parser("read", help="decrypt and describe a cosign.key")
    r.add_argument("--key", required=True)
    r.add_argument("--password")

    s = sub.add_parser("sign", help="ECDSA-P256/SHA-256 ASN.1 DER, base64 to stdout")
    s.add_argument("--key", required=True)
    s.add_argument("--password")
    s.add_argument("--in", dest="infile", required=True)

    v = sub.add_parser("verify")
    v.add_argument("--pub", required=True)
    v.add_argument("--in", dest="infile", required=True)
    v.add_argument("--sig", required=True, help="file holding base64 signature")

    args = ap.parse_args()

    if args.cmd == "generate":
        params = {"legacy": KDF_LEGACY, "standard": KDF_STANDARD,
                  "owasp": KDF_OWASP}[args.kdf]
        _, priv_pem, pub_pem = generate(_pw(args), pem_type=args.pem_type,
                                        kdf_params=params)
        kp, pp = args.out + ".key", args.out + ".pub"
        with open(os.open(kp, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600), "wb") as f:
            f.write(priv_pem)
        with open(pp, "wb") as f:
            f.write(pub_pem)
        print(f"Private key written to {kp}", file=sys.stderr)
        print(f"Public key written to {pp}", file=sys.stderr)

    elif args.cmd == "read":
        with open(args.key, "rb") as f:
            key_pem = f.read()
        priv, der, pem_type = load_private_key(key_pem, _pw(args))
        _, envelope = pem_decode(key_pem)
        d = json.loads(envelope)
        print(json.dumps({
            "pem_type": pem_type,
            "kdf": d["kdf"]["name"],
            "kdf_params": d["kdf"]["params"],
            "salt_len": len(base64.b64decode(d["kdf"]["salt"])),
            "cipher": d["cipher"]["name"],
            "nonce_len": len(base64.b64decode(d["cipher"]["nonce"])),
            "box_len": len(base64.b64decode(d["ciphertext"])),
            "plaintext_pkcs8_der_len": len(der),
            "key_type": type(priv).__name__,
            "curve": getattr(getattr(priv, "curve", None), "name", None),
        }, indent=2))
        sys.stdout.write(priv.public_key().public_bytes(
            encoding=serialization.Encoding.PEM,
            format=serialization.PublicFormat.SubjectPublicKeyInfo).decode())

    elif args.cmd == "sign":
        with open(args.key, "rb") as f:
            priv, _, _ = load_private_key(f.read(), _pw(args))
        with open(args.infile, "rb") as f:
            msg = f.read()
        print(base64.b64encode(sign(priv, msg)).decode())

    elif args.cmd == "verify":
        with open(args.pub, "rb") as f:
            pub = serialization.load_pem_public_key(f.read())
        with open(args.infile, "rb") as f:
            msg = f.read()
        with open(args.sig, "rb") as f:
            sig = base64.b64decode(f.read())
        verify(pub, sig, msg)
        print("Verified OK", file=sys.stderr)


if __name__ == "__main__":
    main()
