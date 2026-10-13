# Real armored-key shapes — the A209-C7 fixture

**Written BEFORE the rule was adjusted**, which is the point of this file.
Seven rounds of this stage produced guards that passed because the fixture
and the rule were built in the same session and the fixture drifted toward
the rule. Round 7's B8-1 guard is the clearest case: it named
`gpg --export-secret-keys --armor` as the format under test and then built
every case as `banner / body / banner` with **no armor header** — the one
shape its own new gate accepted. The live-proof fixture did the same, and
worse: it carried `Proc-Type: 4,ENCRYPTED` *inside* a PGP armor block, a
combination gpg has never emitted and one of exactly three strings the
header allowlist accepted.

## Provenance

`STRUCTURE_MODERN` below is the **verbatim line structure** of output from:

```
gpg (GnuPG) 2.5.20
gpg --batch --passphrase '' --quick-generate-key "…" rsa2048 default 0
gpg --batch --armor --export-secret-keys …
```

run on this machine in a throwaway `GNUPGHOME` that was deleted immediately
after. 33 lines: banner, **blank line**, 27 body lines of exactly 64
characters, a final 52-character line, a 5-character `=CRC` checksum line,
END banner, trailing newline.

The base64 payload is replaced by same-length base64-alphabet lines
carrying `TLCANARY`. Every dimension the detector actually tests — banner
text, the presence and position of headers, the blank line, line widths,
the CRC line, the END banner — is the real thing; only the key bytes are
not, because committing a private key to a repository is not made
acceptable by the key being disposable. This follows
`c3_redaction_corpus.rs`'s standing rule: shape-derived, never copied.

`STRUCTURE_LEGACY` is the RFC 4880 §6.2 header form that GnuPG 1.x and
2.0.x emit and that countless tools still produce:

```
-----BEGIN PGP PRIVATE KEY BLOCK-----
Version: GnuPG v2.0.22 (GNU/Linux)
Comment: https://www.gnupg.org

<base64>
-----END PGP PRIVATE KEY BLOCK-----
```

**RFC 4880 §6.2 armor headers are `Version`, `Comment`, `MessageID`,
`Hash`, `Charset`.** RFC 1421 encapsulated headers are `Proc-Type`,
`DEK-Info`, `Content-Domain`. **The two sets are disjoint**, which is
exactly why a detector that allowlists only the RFC 1421 three lets every
real gpg armor block escape: `Version:` becomes the deciding content line,
fails the base64 test, and the block is emitted whole.

## The nine shapes that must mask

Each masked correctly at some predecessor and leaked at `53046ae`. They are
must-mask rows in `a209_masking_corpus.rs` and assertions in
`a209_b8_reproductions.rs`.

1. Modern gpg armor — banner, blank line, 64-char body.
2. Legacy gpg armor — `Version:` header present.
3. Armor with `Comment:` and `MessageID:` headers.
4. One-line PEM: `-----BEGIN …-----<body>-----END …-----`, no whitespace
   between the banner and the payload.
5. The same key inside JSON with `\n` escapes — the shape of a Google
   service-account key file.
6. Prose before the body (`the key follows`) inside the block.
7. A short first wrap (31 characters) followed by a full 64-character line.
8. A gutter-marked diff deletion (`-` prefix on each line).
9. RSA PEM with RFC 1421 headers — the shape that already worked, kept so
   fixing the others cannot silently drop it.

## The rule this fixture forces

**Scan a window, do not judge a line.** Within 12 lines after the banner,
is there ANY line that is entirely base64 and at least 40 characters?
Everything else is skipped — prose, blanks, gutters, and headers of *any*
RFC. A code comment or a regex list does not contain a 40-character pure
base64 run; a real key body always does.

The window was already measured at 12 lines in round 7 and rejected in
favour of a first-line rule that reached 0 live false positives by breaking
real keys. The correct lever is the payload test, not the window.
