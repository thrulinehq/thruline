# Third-party font

**Press Start 2P** — Copyright (c) 2012 Cody "CodeMan38" Boisclair
(`cody@zone38.net`), with Reserved Font Name "Press Start 2P".

Licensed under the **SIL Open Font License, Version 1.1**:
<https://scripts.sil.org/OFL>

The five `press-start-2p-*.woff2` files beside this note are unicode-range
subsets (latin, latin-ext, cyrillic, cyrillic-ext, greek) extracted from the
`__bundler/manifest` block of `docs/design/ThruLine - The Arcade Truth.html`
and self-hosted so the cabinet makes **zero network calls** — the engine makes
none, and neither does the skin.

**CLOSED 2026-08-06 (A121).** `OFL.txt` beside this note is the font's own
upstream licence file, fetched verbatim from the Google Fonts release for this
family — not approximated from memory, exactly as A83 required. It carries the
full SIL OFL 1.1 text (PREAMBLE, DEFINITIONS, PERMISSION & CONDITIONS,
TERMINATION, DISCLAIMER) and the upstream copyright line.

> **One discrepancy, recorded rather than reconciled.** The upstream file reads
> `Copyright 2012 The Press Start 2P Project Authors (cody@zone38.net)`, where
> this note above says `Cody "CodeMan38" Boisclair`. Both name the same contact.
> `OFL.txt` is authoritative because it is what upstream ships; this note is
> descriptive. Nothing was edited inside `OFL.txt` — editing a licence file to
> match a summary would be the error A83 was guarding against.

---

# Third-party font (second family)

**IBM Plex Sans** and **IBM Plex Mono** — Copyright © 2017 IBM Corp., with
Reserved Font Names "IBM Plex". Licensed under the **SIL Open Font License,
Version 1.1**: <https://scripts.sil.org/OFL>

The 21 `ibm-plex-*.woff2` files beside this note are unicode-range subsets
extracted from the `__bundler/manifest` block of the operator-supplied design
bundle `Thruline Advanced Trace - standalone.html` and self-hosted, for the same
reason as Press Start 2P above: the skin makes **zero network calls**. They are
the typefaces the Advanced Trace design specifies, and `plex.css` beside them is
the `@font-face` block rewritten to point at these local files.

Upstream ships **33 `@font-face` rules over 21 distinct payloads** — Sans
weights 400/500/600 resolve to one file per subset, Mono's three weights are
genuinely distinct. Each payload is therefore stored **once**, named by family,
subset and the first eight hex of its SHA-256, and referenced by every rule that
resolves to it. 230 KB total.

**Every subset is kept, deliberately.** Dropping cyrillic/greek/vietnamese would
save ~90 KB of embedded binary, and would silently break rendering the day a real
transcript carries a non-Latin path, prompt, or commit message. This app renders
the developer's own records, so a missing glyph is a correctness regression, not
a slower page. `unicode-range` means a browser still downloads only what it draws.

> **OPEN, recorded rather than papered over.** `OFL.txt` beside this note is
> **Press Start 2P's** upstream licence file and says so. The verbatim upstream
> OFL for IBM Plex is **not yet checked in**. A83's rule is that a licence file
> is fetched verbatim from the upstream release, never approximated from memory
> or summarised by a tool — so none was written here. Before any distribution
> that carries these files, fetch `LICENSE.txt` from the IBM Plex release
> verbatim and check it in as `OFL-IBM-Plex.txt`.
