# `fixtures/net/` — where each of these came from

Every file here is either **recorded** (a real HTTP response, saved verbatim)
or **constructed** (assembled by hand from real material). The distinction is
load-bearing: a test that passes against an invented response has proved
something about the invention, not about the protocol. Nothing in this
directory is a guess, and anything constructed says so and says out of what.

All recordings were taken **2026-09-04 / 2026-09-05** against Sigstore
**staging**, except the two production Rekor entries, which were read (never
written) from the public production log.

## Recorded verbatim

| file | request | what it proves |
|---|---|---|
| `oidc-staging-openid-configuration.json` | `GET https://oauth2.sigstage.dev/auth/.well-known/openid-configuration` → 200 | where the device and token endpoints actually are; that S256 is offered |
| `oidc-staging-device-code.json` | `POST .../auth/device/code` with `client_id=sigstore`, `scope=openid email`, `code_challenge=VtX6czP210fbQsI5QH5dpMMvTHnzXQkrE0_TWkAtnFw`, `code_challenge_method=S256` → 200 | the documented fields, **`expires_in: 300`**, `interval: 5` |
| `oidc-staging-device-code-no-pkce.json` | the same POST with the two PKCE fields removed → 400 | `"PKCE S256 is required."` — PKCE is mandatory, not preferred |
| `oidc-staging-token-authorization-pending.json` | `POST .../auth/token` polling an un-approved device code → **400** | `authorization_pending` arrives with a 400, so a transport that raises on status cannot poll |
| `oidc-staging-token-invalid-device-code.json` | the same POST with `device_code=nope` → 400 | `"Invalid Device code."` — terminal, and distinguishable from pending |
| `fulcio-staging-configuration.json` | `GET https://fulcio.sigstage.dev/api/v2/configuration` → 200 | `oauth2.sigstage.dev/auth` is `issuerType: email` — the fact the proof-of-possession rule rests on. Also contains the Chainguard entry whose `challengeClaim` is `"sub"` while its `Name()` prefers the email, which is why `challengeClaim` is evidence and not the rule |
| `fulcio-staging-signingcert-400.json` | `POST .../api/v2/signingCert`, well-formed body, unusable token → 400 | the one error body Fulcio returns for a wrong token, a wrong audience, an unknown issuer **and** a proof over the wrong string |
| `fulcio-staging-intermediate.pem`, `fulcio-staging-root.pem` | `GET https://fulcio.sigstage.dev/api/v1/rootCert`, split | the real staging CA chain; P-384 / ecdsa-with-SHA384 |
| `rekor-staging-entry-hashedrekord.json` | `GET https://rekor.sigstage.dev/api/v1/log/entries/6fba9352…c485` → 200 | **the two log indices**: global `24201800`, shard-local `24186981`, tree size `31667593`. Both pass the proof-length check; only the shard-local one reconstructs the checkpoint's root |
| `rekor-staging-entry-hashedrekord-refetch.json` | the **same** GET, taken about an hour later | entries are not byte-stable: the `signedEntryTimestamp` differs while every signed field is identical |
| `rekor-staging-log.json` | `GET https://rekor.sigstage.dev/api/v1/log` → 200 | three retired shards; the fixture entry lives in one of them, not in the active tree |
| `rekor-staging-publickey.pem` | `GET https://rekor.sigstage.dev/api/v1/log/publicKey` → 200 | the staging log's P-256 key |
| `rekor-staging-upload-400.json` | `POST .../api/v1/log/entries` with a non-PEM public key → 400 | `"invalid public key: failure decoding PEM"` |
| `rekor-prod-entry-dsse.json` | `GET https://rekor.sigstore.dev/api/v1/log/entries/108e9186…8b31` → 200 | a real **`dsse` 0.0.1** entry — the kind this crate writes — where the wrong index *is* caught by the length check (21 wanted, 30 present), which is exactly why length is not the defence |

## Re-shaped from real bundles

These two are not HTTP recordings and are not inventions either: every field is
the corresponding field of a real published Sigstore bundle, converted from the
bundle's ProtoJSON encoding (base64 bytes, quoted int64) into Rekor's REST
encoding (lowercase hex, bare numbers) — the inverse of
`net::tlog_entry_from_rekor`. The UUID is recomputed as
`SHA256(0x00 ‖ canonicalizedBody)`, in the 64-character unsharded form.

Each file holds an **envelope and an entry together**, because the pairing is
the fact under test: `rekor::check_entry_names_envelope` compares them, and two
separate files could drift apart while the test went on passing.

| file | source | what it proves |
|---|---|---|
| `rekor-prod-dsse-entry-and-its-envelope.json` | sigstore-conformance `bundle-verify/happy-path-intoto-in-dsse-v3` — a real production entry, `logIndex 155690850`, shard `1193050959916656506`, with its real inclusion proof and checkpoint | that a stored `dsse` body's `spec.payloadHash.value` **is** `sha256` of the decoded DSSE payload. Measured to hold on **11 of 11** real `dsse` tlog entries paired with their own envelopes across sigstore-conformance, sigstore-python and sigstore-rs test corpora |
| `rekor-prod-dsse-entry-and-a-substituted-envelope.json` | sigstore-conformance `bundle-verify/dsse-mismatch-envelope_fail` — upstream's own negative fixture | a real, correctly signed, genuinely-logged entry packaged with an envelope it does not describe. It passes the UUID check *and* the self-consistency check *and* is of the right kind, so it is exactly the substitution none of those can see |

## Constructed, from real material

| file | how | why it is not a recording |
|---|---|---|
| `fulcio-staging-signingcert-embedded-sct.json` | the `CreateSigningCertificateResponse` envelope from `fulcio.proto`, wrapped around the **real** staging leaf, intermediate and root PEMs | obtaining a real 200 needs a human-approved identity token. The certificates are genuine — the leaf really was issued by staging Fulcio on 2026-09-04, validity 21:03:26Z → 21:13:26Z, ten minutes — and the chain really does verify leaf → intermediate → root. Only the JSON wrapper around them is assembled here |
| `fulcio-staging-signingcert-detached-sct.json` | the same three certificates in the `signedCertificateDetachedSct` arm, with a placeholder SCT | public Fulcio only ever emits the embedded arm, so a recording of the detached arm cannot be taken from it at all. The placeholder SCT is base64 of `{"sct_version":0}` and is only ever asserted to have survived parsing — nothing verifies it |

## Not present, and why

There is **no recorded staging `id_token`**, and therefore no recording of a
successful `signingCert` 200 or of a Rekor entry this crate itself wrote.
Completing the device flow requires a person to open a browser and approve;
a test suite cannot. The tokens used in tests are built in
`net::oidc::testing` and say so in their own doc comment. What that leaves
unproven is listed in this module's report rather than inferred from the
pieces around it.
