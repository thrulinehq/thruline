# Vendored: marked 18.0.14 (test instrument only)

`marked.esm.mjs` is the `lib/marked.esm.js` of the npm package `marked@18.0.14`
(a Markdown parser, MIT, MarkedJS), copied byte for byte and renamed `.mjs` only
so that Node loads it as an ES module whatever the package around it says. It
has no imports of its own. It is what `import 'marked'` resolved to in every
measurement of the exported Markdown (EXPORT-MD-ESCAPE round 2's adversary,
which found the two breaks round 3 closes), and it runs with its default options
(`gfm` on), as they did.

    source              https://registry.npmjs.org/marked/-/marked-18.0.14.tgz
    npm integrity       sha512-mBHK6FBHuBAlhgRe88w9F0O1AbwwXJUcQibUbC/QcdTbVGAD7aWza+xt3N6oT/jCZx3/OMeS+8rnuiHZcQ9s7A==
    path in the package lib/marked.esm.js
    sha256 of the file  528a1b88bef88fc27277e06036ce7f4afb6a220b18110ba57c09e292b24a7ce0
    LICENSE sha256      8e3a3f82f59a60958f56ca08f445647c32a4733dc7ca6c2c46f6eb898471ab9c
    vendored by         EXPORT-MD-ESCAPE-R3, 2026-09-30

## Why it is here

Round 2 modelled `marked` (a "quirk model" that read each line of the file for
the shapes `marked` was known to read differently from CommonMark) and never ran
it, because it was not installed in `ui/node_modules`. Two shapes the model did
not know about were live in it, one of them running a script in a browser.
Renderer-quirk hunting cannot be won by a model, so the tests now run the real
renderer, every run, from this file. `ui/test-support/markdown-instrument.mjs`
renders the whole exported file with it, beside CommonMark and `markdown-it`.

## What it is not

It is not loaded by the app, not bundled by `vite build` (nothing under `ui/src`
imports it) and not reachable from `ui/dist` or the binary. The test
`packet-export-md-escape.test.js` recomputes the digest above on every run, so
this file cannot drift from the copy beside it.

## Updating it

Do not edit it in place. Fetch a new release, replace both files, and change the
digests here and in `markdown-instrument.mjs` in the same commit.
