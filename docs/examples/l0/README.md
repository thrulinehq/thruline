# Portable local example

`../../L0-EXAMPLE.html` embeds all four saved canonical status projections and
uses no remote resources. The Python test project was generated for qualification;
its commands actually ran under the optimized candidate identified in
`../../L0-QUALIFICATION.json`. These are synthetic examples, not user validation.

`supported.json`, `failed.json`, `stale.json` and `incomplete.json` preserve the
saved state, scope, counts, applicability, trust, reasons and next action.
Local UI location fields are omitted. The HTML was checked through all four
states, including historical counts and the absence of any counts before a run.

`receipt.json` is the detached signed accepted-work example. `signing.pub` is
only the public key. No signing key, private environment token, output blob or
transcript is included. `offline-verification.json` was produced in a second
empty local environment: signature valid, current evidence unverified, trust
untrusted, and no missing database created. That is the expected portable result
without recognized local evidence; a signature alone cannot establish currentness.

The sample key is not a trusted production identity. Verify with an explicit key:

```sh
thruline verify-receipt docs/examples/l0/receipt.json \
  --key docs/examples/l0/signing.pub --offline --json
```

This command normally exits 2 outside the producing fixture. That is a truthful
unverified result, not a corrupt signature or a passing current-state check.
