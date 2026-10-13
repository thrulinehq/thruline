# Vendored: commonmark.js 0.31.2 (test instrument only)

`commonmark.min.cjs` is the `dist/commonmark.min.js` of the npm package
`commonmark@0.31.2` (the CommonMark reference implementation in JavaScript,
BSD-2-Clause, John MacFarlane), copied byte for byte and renamed `.cjs` only so
that Node loads a UMD file from inside a `"type": "module"` package.

    source              https://registry.npmjs.org/commonmark/-/commonmark-0.31.2.tgz
    npm integrity       sha512-2fRLTyb9r/2835k5cwcAwOj0DEc44FARnMp5veGsJ+mEAZdi52sNopLu07ZyElQUz058H43whzlERDIaaSw4rg==
    file                dist/commonmark.min.js
    sha256 of the file  2de0f8ecbca0a6470da57c8b2ad043777ae999c5132f9abf12e8c332d4e46164
    LICENSE sha256      6cc4b9b28cf68e5bd20f9e94859a85cde35095ef4f9d0f7385b0b6166642f50b
    vendored by         EXPORT-MD-ESCAPE-R2, 2026-09-30

## Why it is here

The exported Markdown (`packet-export.js`) is a file people share. Whether a
piece of recorded text can become a live link, a live tag or a reference
definition is decided by a Markdown renderer, not by a line-by-line reading of
the file (that is how round 1 missed a definition whose label runs over several
lines). `ui/test-support/markdown-instrument.mjs` therefore renders the whole
file and inspects what comes out, and it needs a renderer that is always
present. This is the reference implementation. `marked` and `markdown-it` are
vendored beside it (`../marked/`, `../markdown-it/`, added by
EXPORT-MD-ESCAPE-R3 after round 2 modelled them instead of running them, and two
shapes the model did not know were live in `marked`), and the instrument runs
all three on every check; the test says out loud which ran.

## What it is not

It is not loaded by the app, not bundled by `vite build` (nothing under
`ui/src` imports it) and not reachable from `ui/dist`. The test
`packet-export-md-escape.test.js` recomputes the digest above on every run, so
this file cannot drift from the copy beside it.

## Updating it

Do not edit it in place. Fetch a new release, replace both files, and change
the digests here and in `markdown-instrument.mjs` in the same commit.
