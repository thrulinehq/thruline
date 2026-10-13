# The ThruLine licence file, format 1

T-DIST-4 reads this format; T-DIST-6's issuer writes it. `src/lib.rs` (`parse_envelope`, `seal`, `verify`) is the reference; this page and that code must agree, and `tests/verdicts.rs` round-trips it.

## The file

One UTF-8 text file. Lines end in `\n` (a trailing `\r` is ignored). Empty lines and lines starting with `#` are ignored, so an issuer may add a human note. The first remaining line is exactly:

```
ThruLine licence
```

Then exactly these three fields, each once, in any order:

| Field | Value |
|---|---|
| `key: ` | the key id: the first 8 bytes of SHA-256(`thruline-licence-key-v1\0` ‖ 32-byte Ed25519 public key), 16 lowercase hex characters |
| `payload: ` | the payload: one line of JSON, stored as-is |
| `signature: ` | Ed25519 signature over the **exact payload bytes as stored** (everything after `payload: ` up to the line end), 128 lowercase hex characters |

A field repeated, a field this table does not name, or a missing field makes the file "not a licence". Nothing is ever re-serialised: the verifier checks the signature against the stored bytes and only then parses them.

## The payload

| Key | Type | Meaning |
|---|---|---|
| `format` | integer | `1`. A larger number is refused as "made for a newer version". |
| `licence_id` | string | a ULID |
| `product` | string | `"thruline"` |
| `plan` | string | `"pro"` or `"team"` — the set lives in `PLANS` in `src/lib.rs`. Founder ruling 2 (D-298): Pro is the only plan sold (`SOLD`); `"team"` is dormant — still read and verified, never issued, and never named to a person |
| `issued_at` | string | `YYYY-MM-DDTHH:MM:SSZ`, UTC |
| `expires_at` | string | `YYYY-MM-DDTHH:MM:SSZ`, UTC; the licence stops working at this instant |
| `machine` | string, optional | the machine fingerprint (below); when present the licence works on that machine only |

Unknown payload keys are ignored, so a later issuer can add one without breaking an older binary.

## The machine fingerprint

SHA-256(`thruline-licence-machine-v1\0` ‖ platform id), 64 lowercase hex characters. The platform id is the `IOPlatformUUID` from `ioreg -rd1 -c IOPlatformExpertDevice` on macOS and the trimmed contents of `/etc/machine-id` on Linux. It is derived on the machine and never transmitted by ThruLine; a person reads it with `thruline licence fingerprint` and gives it to the issuer themselves. When no platform id can be read, a licence that names a `machine` is refused with its own sentence. There is no fallback guess.

## Verification order

parse → key known → signature → payload parse and `format` → product → plan known → not expired → `machine`, if present, equals this machine's fingerprint. The first failure decides the sentence a person reads.

## Example (shape only; the values are placeholders, not a real licence)

```
ThruLine licence
# Install with: thruline licence install <this file>
key: <16 hex>
payload: {"format":1,"licence_id":"<ULID>","product":"thruline","plan":"pro","issued_at":"2026-09-23T00:00:00Z","expires_at":"2027-09-23T00:00:00Z"}
signature: <128 hex>
```
