#!/usr/bin/env python3
"""Emit a conforming receipt WITHOUT ThruLine, from the specification alone.

This exists to test one sentence in `execution-check-v0.1.md`:

    "A producer that has never run ThruLine can read it and emit a receipt
     that ThruLine's verifier accepts."

That is a claim about the document, and a claim about a document is only worth
what a check of it is worth. So this file is deliberately written to the *spec*
and not to the Rust types: it does not import anything of ThruLine's, it uses
only the standard library plus `cryptography` for Ed25519, and every section
number in a comment is the section it was written from.

    ./emit_example.py <repo> <out.json>      emit a receipt for a git repo
    ./emit_example.py --selftest             emit into a throwaway repo

Then, to check the claim:

    thruline verify-receipt <out.json> --key <the .pub it wrote>

The expected conclusion is `partial` (exit 2), not `verified`, and that is the
correct answer rather than a shortfall: this producer keeps no ThruLine ledger,
so §7's chain-head step cannot run here and reports "not checked". A verifier
that called that `verified` would be doing the thing the whole format exists to
prevent.
"""
import base64
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import hashlib

from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric import ed25519

PREDICATE_TYPE = "https://thrulinehq.com/attestation/execution-check/v0.1"
STATEMENT_TYPE = "https://in-toto.io/Statement/v1"
PAYLOAD_TYPE = "application/vnd.in-toto+json"


# --- §3.3 canonicalisation ---------------------------------------------------
def jcs(obj) -> bytes:
    """RFC 8785, for the subset of JSON this predicate uses.

    Keys sorted by UTF-16 code unit, no insignificant whitespace, integers
    written without a fraction. The predicate carries no floats and no
    non-BMP keys, so the sort below is the RFC's sort for this input; a
    general implementation needs the full number and code-unit rules.
    """
    return json.dumps(
        obj, sort_keys=True, separators=(",", ":"), ensure_ascii=False
    ).encode("utf-8")


# --- §3.1 PAE ----------------------------------------------------------------
def pae(payload_type: str, payload: bytes) -> bytes:
    t = payload_type.encode()
    return b"DSSEv1 " + str(len(t)).encode() + b" " + t + b" " + str(len(payload)).encode() + b" " + payload


def _pae_test_vector():
    """The spec's own worked vector, checked before anything else is signed."""
    got = pae(PAYLOAD_TYPE, b"{}\n")
    want = b"DSSEv1 28 " + PAYLOAD_TYPE.encode() + b" 3 {}\n"
    assert got == want, f"PAE disagrees with the spec's test vector:\n  {got!r}\n  {want!r}"


def sha256_hex(b: bytes) -> str:
    return "sha256:" + hashlib.sha256(b).hexdigest()


# --- §4.1 the tree hash ------------------------------------------------------
def tree_hash(repo: pathlib.Path) -> str:
    """Everything staged into a TEMPORARY index, never the user's own."""
    # The index path must NOT exist yet. A zero-byte file is not a valid index
    # and `git add` exits 128 on it — so `NamedTemporaryFile`, which creates the
    # file, is the wrong tool here even though it is the obvious one.
    with tempfile.TemporaryDirectory() as d:
        index = str(pathlib.Path(d) / "index")
        env = dict(os.environ, GIT_INDEX_FILE=index)
        subprocess.run(["git", "add", "-A"], cwd=repo, env=env, check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        out = subprocess.run(["git", "write-tree"], cwd=repo, env=env, check=True,
                             capture_output=True, text=True)
        return out.stdout.strip()


def head_commit(repo: pathlib.Path):
    r = subprocess.run(["git", "rev-parse", "HEAD"], cwd=repo, capture_output=True, text=True)
    return r.stdout.strip() if r.returncode == 0 else None


def build(repo: pathlib.Path) -> dict:
    tree = tree_hash(repo)
    commit = head_commit(repo)

    # A real check: run the tests and record what happened. Nothing here is
    # taken from anything the agent said (§0.2).
    argv = ["python3", "-m", "pytest", "-q"]
    proc = subprocess.run(argv, cwd=repo, capture_output=True, text=True)
    passed = proc.stdout.count(" passed")

    digest = {"gitTree": tree}
    if commit:
        digest["gitCommit"] = commit

    predicate = {
        # §5.1 — this producer keeps no hash chain, so the "chain" is the one
        # record it does keep, and its length is 1. §9.1 says to do exactly
        # this rather than to invent a chain.
        "session": {
            "id": "third-party-emitter-example",
            "harness": "generic",
            "event_chain_head": sha256_hex(jcs({"argv": argv, "tree": tree})),
            "event_count": 1,
        },
        "claims": [{
            "id": "clm_example",
            "kind": "tests_pass",
            "text": "All tests pass.",
            "asserted_by": {"harness": "generic"},
        }],
        "checks": [{
            "id": "chk_example",
            "category": "test",
            "runner": "pytest",
            "argv_hash": sha256_hex(jcs(argv)),
            "env_hash": sha256_hex(jcs({"PATH": os.environ.get("PATH", "")})),
            "cwd_hash": sha256_hex(str(repo).encode()),
            "input_tree_hash": tree,
            "exit_code": proc.returncode,
            # §5.3 — never `{}`. This producer has no pytest parser, and says so
            # rather than defaulting to a clean-looking summary.
            "parsed": {"unverifiable": "this producer does not parse pytest output"},
            "reexecuted": False,
        }],
        # §5.4 — the honest verdict for a producer that did not observe the
        # session. `unverifiable`, not `supported`, and never for want of trying.
        "verdicts": [{
            "claim_id": "clm_example",
            "verdict": "unverifiable",
            "verifier": "third_party_example@1",
            "evidence": ["chk_example"],
            "reasons": [
                "the check ran on the subject tree",
                f"exit code {proc.returncode}",
                "this producer does not parse runner output, so it cannot say how many "
                "tests ran — and a claim it cannot size is a claim it cannot decide",
            ],
        }],
        "promises": {"open": [], "fulfilled": []},
        "verifier": {
            "thruline_version": "third-party-emitter/0.1 (not ThruLine)",
            "mode": "local",
            # §5.8 — "no policy" is a stated digest, never an empty string, and
            # never one of the producer's own devising. This is the well-known
            # `absent` constant, computed here from §5.8's construction rather
            # than copied, so a drift in either shows up as a failed verify.
            "policy_hash": sha256_hex(b"thruline.policy.v1\nabsent\n"),
        },
        # §5.7 — this producer read no hooks and installed no shim. It ran one
        # command itself and observed nothing else, so capture is degraded and
        # says so on the receipt's face.
        "capture": {
            "sources": ["third-party-emitter"],
            "redactions_applied": 0,
            "capture_degraded": True,
            # §5.7 — degraded capture must say WHY, and this producer's reason
            # is the honest one: it ran one command and watched nothing else.
            # The token is what a consumer matches on; the sentence is for a
            # person. A bare `true` leaves a reader nothing to search for.
            "degraded_causes": [
                "no_execution_capture: this producer ran one command itself and "
                "observed no hooks, no shim and no other process, so the record "
                "is one command rather than a session"
            ],
        },
    }
    return {
        "_type": STATEMENT_TYPE,
        "subject": [{"name": f"git+{repo}", "digest": digest}],
        "predicateType": PREDICATE_TYPE,
        "predicate": predicate,
    }


def emit(repo: pathlib.Path, out: pathlib.Path) -> pathlib.Path:
    _pae_test_vector()
    statement = build(repo)
    payload = jcs(statement)

    key = ed25519.Ed25519PrivateKey.generate()
    pub = key.public_key()
    raw_pub = pub.public_bytes(serialization.Encoding.Raw,
                               serialization.PublicFormat.Raw)
    sig = key.sign(pae(PAYLOAD_TYPE, payload))

    envelope = {
        "payload": base64.b64encode(payload).decode(),
        "payloadType": PAYLOAD_TYPE,
        "signatures": [{
            "sig": base64.b64encode(sig).decode(),
            "keyid": sha256_hex(raw_pub),
        }],
    }
    out.write_text(json.dumps(envelope, indent=2) + "\n")
    pubpath = out.with_suffix(".pub")
    pubpath.write_bytes(pub.public_bytes(serialization.Encoding.PEM,
                                         serialization.PublicFormat.SubjectPublicKeyInfo))
    print(f"wrote {out}")
    print(f"wrote {pubpath}  (verify with:  thruline verify-receipt {out} --key {pubpath})")
    return out


def selftest():
    _pae_test_vector()
    print("PAE matches the spec's test vector")
    with tempfile.TemporaryDirectory() as d:
        repo = pathlib.Path(d) / "repo"
        (repo / "tests").mkdir(parents=True)
        (repo / "rate.py").write_text("def rate(n):\n    return n * 6.25\n")
        (repo / "tests" / "test_rate.py").write_text(
            "import sys, pathlib\n"
            "sys.path.insert(0, str(pathlib.Path(__file__).parent.parent))\n"
            "from rate import rate\n\n\n"
            "def test_rate():\n    assert rate(2) == 12.5\n")
        for cmd in (["git", "init", "-q", "."], ["git", "config", "user.email", "a@b.c"],
                    ["git", "config", "user.name", "t"], ["git", "add", "-A"],
                    ["git", "commit", "-qm", "base"]):
            subprocess.run(cmd, cwd=repo, check=True,
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        out = emit(repo, pathlib.Path(sys.argv[2] if len(sys.argv) > 2 else "third-party-receipt.json"))
        # It must satisfy the schema this repository publishes.
        here = pathlib.Path(__file__).parent
        rc = subprocess.run([sys.executable, str(here / "validate.py"), str(out)])
        sys.exit(rc.returncode)


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--selftest":
        selftest()
    elif len(sys.argv) == 3:
        emit(pathlib.Path(sys.argv[1]).resolve(), pathlib.Path(sys.argv[2]))
    else:
        sys.exit(__doc__)
