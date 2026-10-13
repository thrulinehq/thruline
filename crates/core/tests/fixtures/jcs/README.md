# RFC 8785 (JCS) reference test vectors

Copied verbatim from the JSON Canonicalization Scheme reference implementation:

    https://github.com/cyberphone/json-canonicalization
    testdata/ at dc406ceaf94b5fa554fcabb92c091089c2357e83

obtained through the `serde_json_canonicalizer` crate's published sources
(`tests/resources/testdata`), which vendors that same tree. Licensed
Apache-2.0 by the upstream project.

Layout:

- `input/*.json`   — the non-canonical input document
- `output/*.json`  — the expected canonical form, UTF-8
- `outhex/*.txt`   — the expected canonical form as space-separated hex bytes

`crates/core/tests/jcs_rfc8785.rs` drives all three against
`thruline_core::spec::canonical::canonical_bytes_of_json`. The hex vectors are
the load-bearing ones: they pin the exact byte sequence, so a UTF-8
normalisation difference or a stray BOM cannot pass by comparing `String`s
that happen to render alike.
