# The redaction corpus — BUILD-SPEC §4.6's regression gate

**1,958 rows. The gate is 1,748 PASS / 210 FAIL, and the 210 must be the same 210.**

This corpus exists because §4.6 is one sentence — *"strip common secret formats …
use a maintained pattern list plus entropy check"* — and that sentence is the
**starting position** of a seventeen-round failure, not its conclusion. Rounds
A209 and A209-C … A209-C15 (`docs/P0-BUILD-BRIEF.md:2662-2903`) each found the
next member of the leak class **by construction rather than by anticipation**,
and each round's own controls tested an easier case than the defect. Every
generator those rounds named is encoded here, so the next implementation meets
the knowledge on day one.

Built by the independent verifier at `919f672`, before §4.6 was written.

---

## Contract

| `expect` | Assertion |
|---|---|
| `mask` | **No 24-character window of any string in `secrets` may survive.** Sliding windows over the whole payload. |
| `nomask` | The text must survive **byte-identical**. |

Two things the gate never consults:

- **The first payload line only.** A209-C11: *"Every prior control asserted that
  the **first** payload line (or the first three) was absent. Both round-11
  regressions were at the body's **END**."*
- **The presence of a `[masked: …]` marker.** A209-C4 B1 found forged markers
  live in the operator's ledger — `[masked: nothing was masked; safe to paste]`
  was a working forgery. A209-C13 found a *reported* mask sitting over a shipped
  key: *"a reported mask over a shipped key is worse than no mask."* The corpus
  carries the forgery as `nomask/forged-marker`.

---

## Provenance of the key fixtures

`keys/` holds **eleven shape-derived fixtures**, not eleven private keys.

Real keys were generated on the verifier's machine in a throwaway directory
(`openssl genrsa` / `genpkey` / `ecparam`, `ssh-keygen`, and
`gpg --export-secret-keys --armor` from a `GNUPGHOME` deleted immediately
after — gpg 2.5.20, which emits **no armor header**, the modern form A209-C8
corrected the record about). `canaryize.py` then replaced the payload bytes.

Every dimension the detector actually tests is the real thing — banner text,
presence and position of armor headers, the blank line, per-line widths, the
`=CRC` line, the END banner, and the trailing base64 padding. Only the key
bytes are not, following the standing rule stated in
`../pgp_armor_shapes.md`: *"committing a private key to a repository is not
made acceptable by the key being disposable … shape-derived, never copied."*

**The substitution was proven lossless for this gate before it was adopted:**
the corpus was generated twice, once from the real keys and once from the
shape-derived ones, and run through the redactor at `919f672`. Both give
1,748 PASS / 210 FAIL with **identical failure identity sets**.

| fixture | structure preserved |
|---|---|
| `rsa2048.pem` | PKCS#8, 26×64 + 20, no headers |
| `ed25519_pkcs8.pem` | **single 64-char line** — the A209-C12 short body |
| `x25519_pkcs8.pem` | single 64-char line |
| `ec_p256_sec1.pem` | `EC PRIVATE KEY`, 64/64/36 |
| `ec_p256_pkcs8.pem` | `PRIVATE KEY`, 64/64/56 |
| `ec_p384.pem` | `EC PRIVATE KEY`, 64×3 + 32 |
| `ssh_ed25519` | `OPENSSH PRIVATE KEY`, **wrap 70** |
| `ssh_rsa` | `OPENSSH PRIVATE KEY`, wrap 70, 25 lines |
| `gpg_secret_armor.asc` | PGP armor: blank line, 64-wraps, `=r3kD` CRC, END banner |
| `cert.pem` | X.509 certificate — must-**not**-mask control |
| `rsa_pub.pem` | SPKI public key — must-**not**-mask control |

`cert.pem` is signed by an **independent** key. This matters and is a live trap
for any sliding-window control: `RSAPrivateKey` and a certificate's
`SubjectPublicKeyInfo` both encode the **same modulus**, so a certificate
generated from the key under test produces ~214 legitimate 24-character window
hits inside the certificate body. The verifier hit exactly this and had to
disprove a false "leak" before adopting the fixture.

---

## Provenance of every corpus family

| Family | Rows | Round that discovered the generator | The finding, in one line |
|---|---|---|---|
| `width/*/terminated` | 99 | **A209-C11** (wrap width), **A209-C12** (short bodies) | Width is not a constant — OpenSSH emits 70, MIME 76, `fold -w` anything. 5 of 7 widths were undetected. Then: *"every PKCS#8 Ed25519 and X25519 key at 40 wrap widths"* leaked — 91 of 318 pairs — and Ed25519 is `ssh-keygen`'s default. |
| `width/*/unterminated-cut{1,2,3}` | 297 | **A209-C13** | A truncated key is still a key. The unterminated path is heuristic and carries the disclosed **136/578** residual. |
| `decoration/*` (29 families) | 464 | **A209-C10** | *"Any per-line prefix defeats body detection."* Ten rounds enumerated **members**; the generator went unnamed for all ten. 14 of 21 realistic decorations leaked a real RSA-2048 key verbatim. Includes A209-C13's timestamp-plus-hard-wrap-at-80 (a GitHub Actions runner turns a 64-char line into 51 + 13). |
| `end-decoration/*` (18 families) | 270 | **A209-C15** | Wrap the **END line alone** in an ANSI colour — what `bat`, `grep --color` and any coloured CI log emit — and the document ships the whole body at `masked=0`. **78 of 312** shapes leaked. |
| `mentioned-end/*` | 160 | **A209-C13** (lowercase), **A209-C15** (ALL-CAPS) | A *mention* of the END banner is not a terminator. 10 of 16 shapes shipped the whole body while reporting `masked=1`. Then C15: *"Decoration iff no lowercase survives"* accepts every shouted sentence, so `WARNING: DO NOT COMMIT ANYTHING THROUGH -----END PRIVATE KEY-----` resurrected the leak, *"differing only in letter case — and every mention control shipped before this round is lowercase."* |
| `transport/*` (15 families) | 135 | **A209-C8** (escape letters), **A209-C9** (decoder wiring), **A209-C15** (ANSI) | JSON-escaped CRLF put a verbatim RSA-2048 key in a persisted capsule with the consent gate open — the defect was one letter wide. Then stock `node -e console.log(...)` output came back byte-identical at `masked=0`. |
| `token/*` (22 vendors × 11 carriers) | 242 | **A209-C** (word boundary), **A209-C2** (`ghu_`/`ghr_`), **A209-C** J05 (two on one line) | A known prefix behind `_` or `.` was invisible: `/tmp/repo/secrets_ghp_….txt` → `masked=0`, byte-identical. `ghu_` and `ghr_` were absent from the table for four rounds while the doc comment three lines above quoted the regex containing them. |
| `straddle/*` | 264 | **A209-C3 BLOCKING 1** | `operations.rs` cut to 120 characters **before** any masker saw the text, so a credential straddling the cut fell under its detector threshold. 46,371 live Bash events exceed 120 characters. |
| `nomask/*` | 20 | **A209-C3** (BID/CERTIFICATE), **A209-C5** (regex fragments), **A209-C10** (digests, ellipsis) | The over-mask direction is a real harm: the PEM repair fired on bare `-----BEGIN` and destroyed 104 of 240 characters of a real ask across **70 of 76,203 sessions**. Later, `is_key_banner` testing *containment* fired on 260 regex patterns and code fragments. |
| singles (7) | 7 | C4, C5-B7-2, C5-B7-4, C10, C11, C12, C15 | banner-with-prefix-on-line · cert-and-key on one line (the `CERTIFICATE` label vetoing the key beside it) · two keys plus a destructive tail that must survive · trailing content in the same field · `-----%42EGIN` bypassing the raw fast-path guard · key beside cert · ANSI beside CJK/accents/OSC/truncated escape (the **panic** the live corpus found and 138 suites missed). |

---

## The 210 disclosed residuals

`expected-failures.txt` lists them by family identity. Three classes only, and
`spec_redaction_corpus.rs::every_disclosed_residual_belongs_to_a_named_class`
refuses anything else, so a leak cannot be parked in the allowlist by adding a
line:

| Class | Rows | Disclosure |
|---|---|---|
| Unknown vendor formats | **168** | A209-C3 *"Vendor scope, enumerated (D4)"*: the table does not know `glpat-`, `sk_live_`, `whsec_`, `hf_`, `npm_`, `dckr_pat_`, `dop_v1_`, `ya29.` |
| Unterminated blocks | **40** | A209-C13/C14/C15 named residual: **136 of 578** unterminated shapes — no END banner in the window, and payload that after transport yields no run ≥ 40, no run ≥ 64, and no width repeating three times |
| Standing exceptions | **2** | A209-C's named N62 (`id="sk-fingerprint-text"`) and N63 (`sk-market-untouched`) — real CSS identifiers whose bodies clear the 16-character minimum |

Everything hardened is clean: **decoration 464/464, end-decoration 270/270,
mentioned-end 160/160, transport 135/135**, and all seven singles.

---

## Regenerating

```
python3 canaryize.py <real-key> <seed>  > keys/<name>     # only to add a key type
python3 gen_corpus.py keys              > corpus.jsonl
```

Then re-run the corpus against the redactor, re-derive `expected-failures.txt`
from the result, and update **both** sha256 pins in
`crates/core/tests/spec_redaction_corpus.rs`. That test fails on any fixture
edit by design: a gate whose corpus is freely editable is A209-C3 BLOCKING 4
(*"the gate protecting the previous fix was satisfiable by a comment"*) one
layer down.

**Changing this corpus is a re-measurement, never a refactor.**
