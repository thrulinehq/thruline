# Vendored: markdown-it 15.0.2 (test instrument only)

`markdown-it.umd.min.cjs` is the `dist/browser/markdown-it.umd.min.js` of the npm
package `markdown-it@15.0.2` (a Markdown parser, MIT, Vitaly Puzrin and Alex
Kocharin), copied byte for byte and renamed `.cjs` only so that Node loads a UMD
file from inside a `"type": "module"` package. It is the package's one
self-contained build: it carries `linkify-it`, `mdurl`, `entities` and `uc.micro`
inside it, so nothing else has to be vendored beside it. (Its last line is a
`sourceMappingURL` comment for a map that is not shipped; Node ignores it.)

    source              https://registry.npmjs.org/markdown-it/-/markdown-it-15.0.2.tgz
    npm integrity       sha512-q4IGxMv56jCqT4OCRCADBoDP3LO4MhmTXjFbphHPXs4g3j9Xg5RDnxqN8IF/3vIWEU+VCnUq+7JUg/cfy2E6Qw==
    path in the package dist/browser/markdown-it.umd.min.js
    sha256 of the file  635972b985228e8af9f0143647c68616b7a3bb09f6946e7e4a52e43dcf5e7be5
    LICENSE sha256      570ffa10225644e9684044e6e90ca5b15604aaf58656f4ce564aa0aceead08e5
    vendored by         EXPORT-MD-ESCAPE-R3, 2026-09-30
    LICENSE extended    EXPORT-MD-ESCAPE-R4, 2026-09-30 (the notices below; the file's digest above is the extended file's)

## Licence notices

The bundle is one file that contains four other packages, and the notices their
licences require were missing from `LICENSE` (the R3 verifier found it; the B2
licence inventory already listed them). `LICENSE` now holds markdown-it's own
notice and, after it, the notice of each package the bundle carries, copied byte
for byte from that package's LICENSE file: `linkify-it` 6.1.0 (MIT, Vitaly
Puzrin), `mdurl` 2.1.0 (MIT, Vitaly Puzrin and Alex Kocharin, with the Node.js
`url` notice its `parse()` is based on, Joyent), `entities` 8.1.0 (BSD-2-Clause,
Felix Böhm) and `uc.micro` 3.0.0 (MIT, Mathias Bynens). The versions are the
ones the measurement below used; markdown-it 15.0.2 declares `linkify-it`
^6.0.0, `mdurl` ^2.1.0, `entities` ^8.0.0 and `uc.micro` ^3.0.0. The test
`packet-export-md-escape.test.js` (R4 (6)) checks that each notice is there.

## Is the bundle the same parser as the package that was measured?

Every earlier measurement imported the unbundled package (`dist/markdown-it.mjs`,
with `linkify-it` 6.1.0, `mdurl` 2.1.0, `entities` 8.1.0, `uc.micro` 3.0.0
beside it). When this file was vendored, it and that package were run over
300 000 random strings built from Markdown's markup characters, bare addresses
and newlines (`html: true, linkify: true`): 300 000 compared, 0 rendered
differently.

## Why it is here

See `../marked/VENDORED.md`: round 2 modelled the renderers and did not run them.
The tests now run this one, every run, with `html: true` and `linkify: true` (raw
HTML passed through and GitHub-style bare addresses made links: the worst case a
reader can be in).

## What it is not

It is not loaded by the app, not bundled by `vite build` (nothing under `ui/src`
imports it) and not reachable from `ui/dist` or the binary. The test
`packet-export-md-escape.test.js` recomputes the digest above on every run.

## Updating it

Do not edit it in place. Fetch a new release, replace both files, and change the
digests here and in `markdown-it`'s entry in `markdown-instrument.mjs` in the same
commit.
