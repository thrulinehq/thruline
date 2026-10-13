#!/usr/bin/env python3
"""Check a receipt against the published execution-check/v0.1 JSON Schema.

The schema is `spec/execution-check-v0.1.schema.json` at the repository root —
the one whose `$id` and `predicateType` a Rust test asserts equal to the
producer's own constants. This script does not carry a schema of its own.

    ./validate.py <receipt-or-statement-or-predicate.json>

Accepts a DSSE envelope, a bare in-toto Statement, or a bare predicate, and
says which one it found. With no argument it runs its own self-test: the
committed example must validate, and ten mutations must each be caught. A
schema that matches everything passes vacuously, so the negative controls are
not optional decoration — they are how this file earns the word "valid".
"""
import base64, json, pathlib, sys

try:
    from jsonschema import Draft202012Validator
except ImportError:
    sys.exit("needs `jsonschema`:  pip install jsonschema")

HERE = pathlib.Path(__file__).parent
# THE SCHEMA LIVES ONCE, AT THE REPOSITORY ROOT, AND IT IS NOT THIS DIRECTORY'S.
#
# `spec/execution-check-v0.1.schema.json` is the published contract: its `$id`
# and `predicateType` are asserted equal to `statement::SCHEMA_ID` and
# `statement::PREDICATE_TYPE` by a Rust test, and ten witnessed mutations hold
# it to the producer. This directory briefly carried a SECOND schema, written
# independently, claiming the SAME `$id` and describing a different document
# (the predicate alone, where the published one describes the whole Statement).
# Two schemas at one identity is worse than either: a consumer resolving that
# URL gets one of them and is silently wrong about the other. It was deleted
# rather than renamed, because the prose spec beside it needs a schema that the
# code is actually held to, and that is this one.
SCHEMA = json.loads((HERE.parent.parent / "spec" / "execution-check-v0.1.schema.json").read_text())
PREDICATE_TYPE = "https://thrulinehq.com/attestation/execution-check/v0.1"


def statement_of(doc):
    """Unwrap a DSSE envelope to the Statement the schema describes.

    The published schema validates the whole in-toto Statement, not the
    predicate alone — which is stricter, because it also checks `_type`,
    `subject` and `predicateType`, three fields a predicate-only schema
    structurally cannot see.
    """
    if "payload" in doc and "payloadType" in doc:
        if doc["payloadType"] != "application/vnd.in-toto+json":
            sys.exit(f"payloadType is {doc['payloadType']!r}, not an in-toto statement")
        return json.loads(base64.b64decode(doc["payload"])), "DSSE envelope"
    if "predicateType" in doc:
        return doc, "in-toto Statement"
    sys.exit("this is neither a DSSE envelope nor an in-toto Statement — the "
             "published schema validates the Statement, so a bare predicate "
             "cannot be checked against it")


def errors(statement):
    return sorted(Draft202012Validator(SCHEMA).iter_errors(statement), key=lambda e: list(e.path))


def selftest():
    Draft202012Validator.check_schema(SCHEMA)
    good, layer = statement_of(json.loads((HERE / "example-receipt.json").read_text()))
    errs = errors(good)
    if errs:
        for e in errs:
            print(f"  {list(e.path)}: {e.message}")
        sys.exit("the committed example does not validate — the schema and the example disagree")
    print(f"example-receipt.json ({layer}) validates")

    mutations = [
        ("reexecuted:true with no image digest", lambda p: p["predicate"]["checks"][0].update(reexecuted=True)),
        ("image digest with reexecuted:false", lambda p: p["predicate"]["checks"][0].update(container_image_digest="sha256:" + "a" * 64)),
        ("parsed:{}", lambda p: p["predicate"]["checks"][0].update(parsed={})),
        ("empty policy_hash", lambda p: p["predicate"]["verifier"].update(policy_hash="")),
        ("absent policy_hash", lambda p: p["predicate"]["verifier"].pop("policy_hash")),
        ("verdict outside the enum", lambda p: p["predicate"]["verdicts"][0].update(verdict="passed")),
        # KNOWN GAP, filed as IR-1 in docs/tracks/D/INTERFACE-REQUESTS.md. The
        # schema does not constrain `verifier`, so this mutation is NOT caught.
        # It stays in the suite as a tripwire: when the core lands IR-1 this
        # flips to `caught` and the run fails, so the gap cannot quietly close
        # with the request left standing.
        ("verifier with no version", lambda p: p["predicate"]["verdicts"][0].update(verifier="tests_pass"), "IR-1"),
        ("content_filters with capture_degraded false", lambda p: p["predicate"]["capture"].update(content_filters="filter=lfs")),
        ("uppercase hex chain head", lambda p: p["predicate"]["session"].update(event_chain_head="sha256:" + "A" * 64)),
        ("finding with no severity", lambda p: p["predicate"]["verdicts"][0]["findings"][0].pop("severity")),
    ]
    missed, closed = [], []
    for entry in mutations:
        name, mutate = entry[0], entry[1]
        known_gap = entry[2] if len(entry) > 2 else None
        p = json.loads(json.dumps(good))
        mutate(p)
        caught = bool(errors(p))
        if known_gap and not caught:
            print(f"  KNOWN GAP ({known_gap})  {name}")
        elif known_gap:
            closed.append(f"{name} ({known_gap})")
            print(f"  caught  {name}  <-- {known_gap} HAS LANDED; retire the gap")
        elif caught:
            print(f"  caught  {name}")
        else:
            missed.append(name)
            print(f"  MISSED  {name}")
    if errors(json.loads(json.dumps(good))):
        sys.exit("positive control broken: the unmutated example stopped validating")
    print("  positive control: the unmutated example still validates")
    if missed:
        sys.exit(f"{len(missed)} mutation(s) went undetected: {missed}")
    if closed:
        sys.exit(
            "a known gap is now caught, so the request describing it is stale: "
            f"{closed}. Retire it from docs/tracks/D/INTERFACE-REQUESTS.md and "
            "marker here."
        )
    gaps = sum(1 for e in mutations if len(e) > 2)
    print(f"{len(mutations)} mutations: {len(mutations) - gaps} caught, "
          f"{gaps} known gap(s) filed as interface requests")
    spec_example_matches()


def spec_example_matches():
    """§8 of the prose spec must show the committed example, byte for byte.

    A worked example that has drifted from the artifact it claims to be is
    worse than no example: a third-party implementer types it in and it does
    not validate, and they have no way to tell which of the two is wrong.
    """
    spec = (HERE / "execution-check-v0.1.md").read_text()
    anchor = "The decoded payload:\n\n```json\n"
    s = spec.index(anchor) + len(anchor)
    e = spec.index("\n```", s)
    shown = json.loads(spec[s:e])
    committed = json.loads((HERE / "example-statement.json").read_text())
    if shown != committed:
        sys.exit("§8's example and example-statement.json have drifted apart")
    # AND against the receipt's own decoded payload. Comparing the block only
    # to the statement left a hole a real divergence sat in: the two committed
    # files came from different runs and disagreed on `subject.name`, and this
    # self-test could not see it because it never decoded the receipt.
    decoded, _ = statement_of(json.loads((HERE / "example-receipt.json").read_text()))
    if shown != decoded:
        diff = [k for k in set(shown) | set(decoded) if shown.get(k) != decoded.get(k)]
        sys.exit(
            "§8's example and the RECEIPT's decoded payload disagree on "
            f"{diff} — they are not from the same run, so the signature in the "
            "block does not cover the statement printed under it"
        )
    if shown["predicateType"] != PREDICATE_TYPE:
        sys.exit(f"§8's example carries {shown['predicateType']!r}")
    print("§8's example equals the committed statement AND the receipt's decoded payload")


def main():
    if len(sys.argv) == 1:
        return selftest()
    statement, layer = statement_of(json.loads(pathlib.Path(sys.argv[1]).read_text()))
    errs = errors(statement)
    print(f"read a {layer}")
    for e in errs:
        print(f"  {'.'.join(str(x) for x in e.path) or '<root>'}: {e.message}")
    if errs:
        sys.exit(f"{len(errs)} schema violation(s)")
    print("valid against execution-check/v0.1 — shape only; §6's cross-field invariants are a verifier's job")


if __name__ == "__main__":
    main()
