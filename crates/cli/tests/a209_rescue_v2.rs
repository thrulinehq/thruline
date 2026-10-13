//! A209 — rescue gains the sealed canonical v2 capsule beside v1 (MANDATE-v4
//! §15, §11 CAPSULE_CREATED/CAPSULE_OFFERED, §34), through the compiled
//! binary. Staging is an offer: nothing here claims anything about a
//! successor, and the wording walk holds the new stdout lines to that.

use std::path::Path;
use std::process::{Command, Output};

use thruline_core::chain::sha256_hex;
use thruline_core::ledger::Ledger;

fn thruline(db: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_thruline"))
        .arg("--db")
        .arg(db)
        .args(args)
        .output()
        .expect("thruline runs")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// One project with a recorded ask, a supported claim, an unresolved edit
/// run, and (optionally) a resolving repository identity. `with_secret`
/// plants the canary that forces the consent gate.
fn seed(db: &Path, with_identity: bool, with_secret: bool) {
    let ask = if with_secret {
        "wire the hook — key AKIAIOSFODNN7EXAMPLE is in the env"
    } else {
        "wire the hook end to end"
    };
    seed_with_ask(db, with_identity, ask);
}

/// `seed` with the opening ask supplied. A209-C11: the key-material
/// control needs a real PEM in the ask, and `events` is append-only — the
/// trigger correctly refuses an UPDATE — so it has to arrive at insert
/// time. The ask is BOUND, not formatted into the statement.
fn seed_with_ask(db: &Path, with_identity: bool, ask: &str) {
    let ledger = Ledger::open(db).expect("open");
    ledger
        .conn()
        .execute_batch(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
               VALUES ('p1','/tmp/p1','claude_code','p1','2026-09-01T00:00:00Z');
             INSERT INTO sessions(id, project_id, file_path, started_at, last_seq)
               VALUES ('s1','p1','/t/s1.jsonl','2026-09-01T00:00:00Z',3);",
        )
        .expect("seed scope");
    ledger
        .conn()
        .execute(
            "INSERT INTO events(id, project_id, session_id, seq, ts, kind, text, tool_name,
                                tool_input, raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
             VALUES ('e1','p1','s1',0,'2026-09-01T00:00:01Z','user_msg',?1,
                     NULL,NULL,'/t/s1.jsonl',1,'x','g','c1')",
            [ask],
        )
        .expect("seed ask");
    ledger
        .conn()
        .execute_batch(
            "INSERT INTO events(id, project_id, session_id, seq, ts, kind, text, tool_name,
                                tool_input, raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
               VALUES ('e2','p1','s1',1,'2026-09-01T00:00:02Z','tool_use',NULL,'Bash',
                       '{\"command\":\"git commit -m seed\"}','/t/s1.jsonl',2,'x','c1','c2'),
                      ('e3','p1','s1',2,'2026-09-01T00:00:03Z','tool_use',NULL,'Edit',
                       '{\"file_path\":\"src/lib.rs\"}','/t/s1.jsonl',3,'x','c2','c3'),
                      ('e4','p1','s1',3,'2026-09-01T00:00:04Z','assistant_text','done',
                       NULL,NULL,'/t/s1.jsonl',4,'x','c3','c4');
             INSERT INTO nodes(id, project_id, label, sort_key, lifecycle, evidence_state,
                               currency, session_id, anchor_event_id, derived_rev)
               VALUES ('n1','p1','hook wired','supported','completed','supported','current',
                       's1','e4',1);",
        )
        .expect("seed");
    if with_identity {
        ledger
            .conn()
            .execute_batch(
                "INSERT INTO workspace_identities(project_id, repo_id, basis, observed_at)
                   VALUES ('p1','repo_p1','git','2026-09-01T00:00:00Z');",
            )
            .expect("identity");
    }
}

fn observation_count(db: &Path, kind: &str) -> i64 {
    let conn = rusqlite::Connection::open(db).expect("open db");
    conn.query_row(
        "SELECT COUNT(*) FROM observations WHERE observation_type = ?1",
        [kind],
        |r| r.get(0),
    )
    .expect("count")
}

#[test]
fn rescue_stages_the_sealed_v2_capsule_beside_v1_and_records_the_offer() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db = tmp.path().join("ledger.db");
    seed(&db, true, false);
    let out_dir = tmp.path().join("rescue");

    let run = thruline(
        &db,
        &[
            "rescue",
            "--project",
            "p1",
            "--out",
            out_dir.to_str().expect("utf8"),
        ],
    );
    assert!(run.status.success(), "{run:?}");
    let text = stdout(&run);

    // v1 is not lost: both of its artifacts still stage.
    assert!(out_dir.join("capsule.json").exists(), "v1 json staged");
    assert!(out_dir.join("capsule.md").exists(), "v1 markdown staged");
    assert!(
        out_dir.join("rescue-settings.json").exists(),
        "injection staged"
    );

    // The v2 capsule is staged byte-identical to its sealed bytes: the
    // staged file's own sha256 IS the sealed digest the banner prints.
    let v2_bytes = std::fs::read(out_dir.join("capsule-v2.json")).expect("v2 staged");
    let digest = sha256_hex(&v2_bytes);
    assert!(
        text.contains(&format!("digest {}…", &digest[..16])),
        "the printed digest is the staged file's own sha256: {text}"
    );
    let capsule = thruline_core::capsule_v2::open_sealed_bytes(&v2_bytes)
        .expect("the staged v2 capsule parses and its id verifies");
    assert_eq!(capsule.scope.id, "p1");
    assert!(capsule.repository.resolves);
    assert_eq!(
        capsule.open_obligations.len(),
        1,
        "the unresolved run transfers"
    );
    assert!(
        capsule.open_obligations[0].item_id.starts_with("obl1:"),
        "with its stable id"
    );
    assert!(out_dir.join("capsule-v2.md").exists(), "v2 markdown staged");

    // The sealed bytes are ALSO in the protected store, content-addressed
    // under the same digest.
    let store = thruline_core::protected_store::ProtectedStore::open(tmp.path().join("protected"))
        .expect("store");
    assert_eq!(store.get(&digest).expect("sealed blob"), v2_bytes);

    // Both observations landed, once each, with the digest in the receipt.
    assert_eq!(observation_count(&db, "CAPSULE_CREATED"), 1);
    assert_eq!(observation_count(&db, "CAPSULE_OFFERED"), 1);
    assert!(text.contains("observed      CAPSULE_CREATED"), "{text}");
    assert!(text.contains("observed      CAPSULE_OFFERED"), "{text}");
    let conn = rusqlite::Connection::open(&db).expect("open db");
    let receipt: String = conn
        .query_row(
            "SELECT source_receipt FROM observations WHERE observation_type='CAPSULE_OFFERED'",
            [],
            |r| r.get(0),
        )
        .expect("receipt");
    assert!(receipt.contains(&digest), "the receipt pins the digest");
    assert!(
        receipt.contains("\"offered_to_agent\":\"claude\""),
        "{receipt}"
    );

    // The wording walk, c43-style, over the new surface: staging is an
    // offer, and no stdout line claims more than an offer. The literal
    // verb name `verify-continuity` is v1's pre-existing legacy pointer
    // (a command name, not a claim) and is excluded by exact token; any
    // OTHER continuity word still fails the walk.
    let text = text.replace("verify-continuity", "");
    for banned in [
        "continuity",
        "recovered",
        "resumed",
        "seamless",
        "successor accepted",
        "handoff complete",
    ] {
        assert!(
            !text.to_lowercase().contains(banned),
            "rescue's stdout says `{banned}` — a claim no observation backs"
        );
    }
}

#[test]
fn a_second_rescue_seals_a_new_capsule_linked_by_lineage() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db = tmp.path().join("ledger.db");
    seed(&db, true, false);
    let out1 = tmp.path().join("r1");
    let out2 = tmp.path().join("r2");

    let first = thruline(
        &db,
        &[
            "rescue",
            "--project",
            "p1",
            "--out",
            out1.to_str().expect("utf8"),
        ],
    );
    assert!(first.status.success(), "{first:?}");
    let c1 = thruline_core::capsule_v2::open_sealed_bytes(
        &std::fs::read(out1.join("capsule-v2.json")).expect("v2"),
    )
    .expect("verifies");
    assert_eq!(c1.lineage, None, "the first capsule has no prior");

    let second = thruline(
        &db,
        &[
            "rescue",
            "--project",
            "p1",
            "--out",
            out2.to_str().expect("utf8"),
        ],
    );
    assert!(second.status.success(), "{second:?}");
    let c2 = thruline_core::capsule_v2::open_sealed_bytes(
        &std::fs::read(out2.join("capsule-v2.json")).expect("v2"),
    )
    .expect("verifies");
    assert_ne!(
        c2.capsule_id, c1.capsule_id,
        "a new sealing is a new capsule"
    );
    assert_eq!(
        c2.lineage.as_deref(),
        Some(c1.capsule_id.as_str()),
        "the second capsule names the first as lineage"
    );
    assert!(
        stdout(&second).contains("supersedes"),
        "the lineage is stated in the banner"
    );

    // The first capsule's staged bytes still verify — sealing the second
    // touched nothing about the first.
    let again = thruline_core::capsule_v2::open_sealed_bytes(
        &std::fs::read(out1.join("capsule-v2.json")).expect("v2"),
    )
    .expect("still verifies");
    assert_eq!(again, c1);
}

/// The gate is a UNION, exercised with one arm genuinely FALSE (A209
/// correction: the original test planted a canary both compilers mask, so
/// the `||` never had a false arm and inspection was doing the work).
///
/// The asymmetry is structural, not contrived: v1 masks the scope's
/// `display_name`; the v2 document has no display-name field at all, so a
/// canary planted only there makes v1's treatment require acknowledgment
/// while v2's matches nothing. The gate must still close.
#[test]
fn the_consent_gate_closes_when_only_one_compilers_treatment_requires_it() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db = tmp.path().join("ledger.db");
    seed(&db, true, false); // no canary in the transcript
    {
        let ledger = Ledger::open(&db).expect("open");
        ledger
            .conn()
            .execute(
                "UPDATE projects SET display_name = ?1 WHERE id = 'p1'",
                ["proj AKIAIOSFODNN7EXAMPLE"],
            )
            .expect("plant the canary in the display name only");
    }
    let out_dir = tmp.path().join("rescue");
    let blocked = thruline(
        &db,
        &[
            "rescue",
            "--project",
            "p1",
            "--out",
            out_dir.to_str().expect("utf8"),
        ],
    );
    assert!(blocked.status.success(), "{blocked:?}");
    let text = stdout(&blocked);

    // The asymmetry is real: one scan reports a mask, the other reports none.
    let v1_line = text
        .lines()
        .find(|l| l.trim_start().starts_with("secret scan"))
        .expect("v1 scan line");
    let v2_line = text
        .lines()
        .find(|l| l.trim_start().starts_with("v2 scan"))
        .expect("v2 scan line");
    assert!(
        v1_line.contains("1 masked"),
        "v1 should have masked the display name: {v1_line}"
    );
    assert!(
        v2_line.contains("nothing matched"),
        "v2 does not carry a display name, so its arm is false: {v2_line}"
    );

    // One arm false, and the gate still closes.
    assert!(
        text.contains("INJECTION BLOCKED"),
        "the union gate opened on a single-arm finding: {text}"
    );
    assert!(!out_dir.exists(), "nothing staged without acknowledgment");
    assert_eq!(observation_count(&db, "CAPSULE_CREATED"), 0);
    assert_eq!(observation_count(&db, "CAPSULE_OFFERED"), 0);
}

#[test]
fn the_consent_gate_blocks_both_capsules_and_stages_neither() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db = tmp.path().join("ledger.db");
    seed(&db, true, true); // the canary forces masking → the gate closes
    let out_dir = tmp.path().join("rescue");

    let blocked = thruline(
        &db,
        &[
            "rescue",
            "--project",
            "p1",
            "--out",
            out_dir.to_str().expect("utf8"),
        ],
    );
    assert!(blocked.status.success(), "{blocked:?}");
    let text = stdout(&blocked);
    assert!(text.contains("INJECTION BLOCKED"), "{text}");
    assert!(
        text.contains("v2 scan"),
        "the v2 treatment is surfaced before consent: {text}"
    );
    assert!(!out_dir.exists(), "nothing staged without acknowledgment");
    assert_eq!(observation_count(&db, "CAPSULE_CREATED"), 0);
    assert_eq!(observation_count(&db, "CAPSULE_OFFERED"), 0);

    // Acknowledged: staged, sealed, recorded — and the canary is not in
    // any staged byte.
    let staged = thruline(
        &db,
        &[
            "rescue",
            "--project",
            "p1",
            "--out",
            out_dir.to_str().expect("utf8"),
            "--acknowledge",
        ],
    );
    assert!(staged.status.success(), "{staged:?}");
    for artifact in [
        "capsule.json",
        "capsule.md",
        "capsule-v2.json",
        "capsule-v2.md",
    ] {
        let bytes = std::fs::read(out_dir.join(artifact)).expect(artifact);
        let text = String::from_utf8_lossy(&bytes);
        assert!(
            !text.contains("AKIAIOSFODNN7EXAMPLE"),
            "the canary reached {artifact}"
        );
    }
    assert_eq!(observation_count(&db, "CAPSULE_CREATED"), 1);
}

#[test]
fn an_unresolved_scope_stages_the_capsule_and_abstains_from_recording() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db = tmp.path().join("ledger.db");
    seed(&db, false, false); // no workspace_identities row
    let out_dir = tmp.path().join("rescue");

    let run = thruline(
        &db,
        &[
            "rescue",
            "--project",
            "p1",
            "--out",
            out_dir.to_str().expect("utf8"),
        ],
    );
    assert!(run.status.success(), "{run:?}");
    let text = stdout(&run);
    // The capsule itself stages — it states its own unresolved scope.
    let capsule = thruline_core::capsule_v2::open_sealed_bytes(
        &std::fs::read(out_dir.join("capsule-v2.json")).expect("v2"),
    )
    .expect("verifies");
    assert!(!capsule.repository.resolves);
    // The writers abstain rather than record against a guess, and say so.
    assert!(
        text.contains("not recorded — "),
        "the abstention is printed: {text}"
    );
    assert_eq!(observation_count(&db, "CAPSULE_CREATED"), 0);
    assert_eq!(observation_count(&db, "CAPSULE_OFFERED"), 0);
}

/// B8-5 / B7-5: the consent gate, exercised through the REAL `rescue`
/// path — the test that never existed.
///
/// The previous control asserted on a `MaskRecord` the test itself held
/// from `mask_recorded`; it never called `compile` or `rescue`, so it
/// could not see a record failing to REACH the treatment — which is the
/// entire defect. At `90806e0` a credential masked into `nodes.label`
/// upstream produced `Ran{masked:0}` "nothing matched", `needs_ack` false,
/// and `rescue` staged all four capsules plus `rescue-settings.json` with
/// no `--acknowledge`.
#[test]
fn the_consent_gate_closes_when_a_label_was_masked_upstream() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db = tmp.path().join("ledger.db");
    seed(&db, true, false);
    {
        let ledger = Ledger::open(&db).expect("open");
        ledger
            .conn()
            .execute(
                "UPDATE nodes SET label = ?1 WHERE id = 'n1'",
                ["deployed with [masked: aws] credentials"],
            )
            .expect("plant an upstream-masked label");
    }
    let out_dir = tmp.path().join("rescue");
    let run = thruline(
        &db,
        &[
            "rescue",
            "--project",
            "p1",
            "--out",
            out_dir.to_str().expect("utf8"),
        ],
    );
    assert!(run.status.success(), "{run:?}");
    let text = stdout(&run);
    assert!(
        text.contains("INJECTION BLOCKED"),
        "the gate OPENED over a capsule containing a masked span: {text}"
    );
    assert!(
        !out_dir.exists(),
        "artifacts were staged without acknowledgment"
    );
    assert_eq!(observation_count(&db, "CAPSULE_CREATED"), 0);
    assert_eq!(observation_count(&db, "CAPSULE_OFFERED"), 0);
}

/// A209-C11 — a REAL private key, narrowly wrapped, through the REAL
/// `rescue` path, asserted absent from BOTH compilers' artifacts.
///
/// Every previous end-to-end control used a 64-column key, which is the
/// one width the detection bar was tuned to. A key wrapped at 40 has no
/// run of 64 anywhere; at `8d93d94` it was not detected at all, and the
/// operator's own ledger is the population this protects. The assertion
/// is total — no 24-character window of the body may appear in any staged
/// file, v1 or v2, json or markdown.
mod keymat_cli {
    /// The same real RSA-2048 key the core controls use, re-wrapped.
    pub const KEY_BODY: &str = "MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQC4wporLzyGqLdsHaQjEBFQyeodUcjRzeZLuNCEyHz0mSjrQBX6XhVsJ5cqGmG3rhYuH8l/8rvolp6mmfQB//QHMoC6tcQUpeAw5kHQDywq4OwGFPZds8qylzTPQW7nU01q9b+oW15+yIy2AWkBp1Z0UMMFN63UEcnG3ayPwqUdfTUcM7OeWd5iqxV0w2SXyzTyoztRx5e/FyU3K0iN7gsKHVe04WpuXRQgr/XXUfMvzPjc+eT/aI3dEL/eMu0ymwOYp1xaDDtE0Cj/F4FO+BMzho0/+hDxRevEVHQbiKnOhF2Ah4oVZS4+uL7WVttrdOX7E65zEGSc770AkdskcAxNAgMBAAECggEAA4liFxqH35ICNNUaa7tmgXPkAdyraey1kFSA1VHbJnl6EYrAAPNlGJVnbNxla3UvIoHkQwk79JdRgSHoTFOAcdTTJv/MuZJ1SKpXrypjL4zvp6wmDSQgaKvBTIhVnaL4E+iRWyEonWesXd2IC4qO+poDKannr2wJvz9GCthwfbB78lzqKT64ZYFXL4FlvHOIpu/FQ/nDiBWadsdIvdKJApjVeNBaFNI+Fm9LXDLizgswThMN9nXoH3eG9KkJHrUVUzvwoMuLcqbLMkSDCbMhacS6M8efb8vN5+HEE2o+orlaMWzw7lRIIkOpf7bsh2AL4rL2PuLluSRZHfOxUuOskQKBgQD+hfJIKckToAmeaLA7wTujdAE9c6RZLjDUKNkEAurN3/vZSelVHUi9jN0+T9MhIuDLuq9pOf+7F48hjTdH5f50WEGTQmm/wzTO5ZKivMZrucwG8JVm3ifQcTDpHQfhV0S3lqACMYNp0zvcgevp0x6XnG0mSeMsvA3bsv+JQCyf6QKBgQC51QivRkRXNpfv4qH0uvT3vMlVxQgaK9wafhk6N0oVcmbcqngn2fDzkc55ME8rpkdspYidIfrQp5SCoJH8FFmel/1VCH+jxOYsRkTVJBD7D5YAv3X3OlnbT/DXYTSN58sKqk+ICf+rndTidPtfUjmvC+7iY4KY50u8ts5cYmBOxQKBgQCPZHM9CJ2J+u2wWxZ+c7zft2lDb7U1sPo9bM3mnqYeWoQOxL85/oeOkp/KjTpLPtUashBE3jIe+4TdyCtOzFpwvZAkIJRnPtivf1L/cA7xffleJ4hkLeCMZFjD3GEmKP5D2A5Jbg+1PRZKgr+gOhrQG4a6vYHJICFGly+z6X26kQKBgQCjuq0SMOjZpoIrmxRy2Z4oAC+r4BOJa4db2CpF/pzJVKiIV7HR+tWu5JJ0eDnpDXH0qcw6Fcw0ttHEXdEBtrRXZTaRCxHaqwrZi3fuCv7jFRJmykoXb1PoRQC/4yc4uDdOyEuWYqje/gqpE8S+cWXdcrggwug36+Hw/WfIQez0uQKBgEh15dr+T4uR42dDdsb2DukUnJ8XZgcjmRz6HIr6MKbq236q+qi4arqGyIYQtDUFio9Rjsh5uBd7fjow4oMwxYCMo5fBhda5ERmDsKHYBsgPTHXtscYgzBHSFFIqht7UpnXtLSDGvt6beru19NHlUfocB21M+zB+Rj4/f3UQilGJ";

    /// The key re-wrapped at an arbitrary column.
    pub fn rewrap(w: usize) -> String {
        let mut out = String::from("-----BEGIN PRIVATE KEY-----\n");
        let mut i = 0;
        while i < KEY_BODY.len() {
            let end = (i + w).min(KEY_BODY.len());
            out.push_str(&KEY_BODY[i..end]);
            out.push('\n');
            i = end;
        }
        out.push_str("-----END PRIVATE KEY-----");
        out
    }

    /// The first surviving 24-character window of the body, if any.
    pub fn surviving_fragment(out: &str) -> Option<String> {
        const FRAG: usize = 24;
        (0..=(KEY_BODY.len() - FRAG))
            .map(|i| &KEY_BODY[i..i + FRAG])
            .find(|w| out.contains(*w))
            .map(|w| w.to_string())
    }

    /// A real PKCS#8 Ed25519 private key — `ssh-keygen`'s default
    /// algorithm — whose body is ONE line of 64 characters. Re-wrapped
    /// anywhere from 24 to 63 it has no run of 64 and cannot repeat a
    /// width three times, so the round-12 rule could not see it at all.
    pub const ED25519_BODY: &str =
        "MC4CAQAwBQYDK2VwBCIEIB670Y9rX2WANBNk9dFSpprFWswqdWcQOVqJPKSa4q3c";

    pub fn ed25519_rewrapped(w: usize) -> String {
        let mut out = String::from("-----BEGIN PRIVATE KEY-----\n");
        let mut i = 0;
        while i < ED25519_BODY.len() {
            let end = (i + w).min(ED25519_BODY.len());
            out.push_str(&ED25519_BODY[i..end]);
            out.push('\n');
            i = end;
        }
        out.push_str("-----END PRIVATE KEY-----");
        out
    }

    pub fn surviving_ed25519(out: &str) -> Option<String> {
        const FRAG: usize = 24;
        (0..=(ED25519_BODY.len() - FRAG))
            .map(|i| &ED25519_BODY[i..i + FRAG])
            .find(|w| out.contains(*w))
            .map(|w| w.to_string())
    }
}

#[test]
fn a_narrowly_wrapped_real_key_never_reaches_a_staged_artifact_in_either_compiler() {
    for width in [40usize, 56, 64] {
        let tmp = tempfile::tempdir().expect("tempdir");
        let db = tmp.path().join("ledger.db");
        seed_with_ask(
            &db,
            true,
            &format!(
                "rotate this before shipping:\n{}",
                keymat_cli::rewrap(width)
            ),
        );
        let out_dir = tmp.path().join("rescue");

        // The gate must close first: a capsule built over a masked span is
        // never injected without acknowledgment.
        let blocked = thruline(
            &db,
            &[
                "rescue",
                "--project",
                "p1",
                "--out",
                out_dir.to_str().expect("utf8"),
            ],
        );
        assert!(blocked.status.success(), "{blocked:?}");
        assert!(
            stdout(&blocked).contains("INJECTION BLOCKED"),
            "wrap {width}: the gate opened over a capsule built from a real private key: {}",
            stdout(&blocked)
        );

        // Acknowledged, the artifacts stage — and none of them may carry
        // any part of the key, in either compiler's output.
        let run = thruline(
            &db,
            &[
                "rescue",
                "--project",
                "p1",
                "--out",
                out_dir.to_str().expect("utf8"),
                "--acknowledge",
            ],
        );
        assert!(run.status.success(), "{run:?}");
        let mut checked = 0;
        for name in [
            "capsule.json",
            "capsule.md",
            "capsule-v2.json",
            "capsule-v2.md",
            "rescue-settings.json",
        ] {
            let path = out_dir.join(name);
            if !path.exists() {
                continue;
            }
            checked += 1;
            let body = std::fs::read_to_string(&path).expect("read artifact");
            assert!(
                keymat_cli::surviving_fragment(&body).is_none(),
                "wrap {width}: {name} shipped key material: {:?}",
                keymat_cli::surviving_fragment(&body)
            );
        }
        assert!(
            checked >= 4,
            "wrap {width}: expected v1 and v2 artifacts to stage, saw {checked}"
        );
        // The banner the operator reads must not carry it either.
        assert!(
            keymat_cli::surviving_fragment(&stdout(&run)).is_none(),
            "wrap {width}: the rescue banner printed key material"
        );
    }
}

/// A209-C12 — the key type the RSA-only control could not catch.
///
/// At `823f02a` this failed with the consent gate OPEN: an Ed25519 key
/// wrapped at 40 in the opening ask produced no detection, so `rescue`
/// staged both compilers' artifacts with no acknowledgment and the whole
/// 64-character body was present in all four capsules. RSA-2048 has 26
/// body lines and always cleared the "3 lines of equal width" bar;
/// Ed25519 has one line and never can.
#[test]
fn a_short_bodied_key_never_reaches_a_staged_artifact_in_either_compiler() {
    for width in [24usize, 40, 63, 64] {
        let tmp = tempfile::tempdir().expect("tempdir");
        let db = tmp.path().join("ledger.db");
        seed_with_ask(
            &db,
            true,
            &format!(
                "rotate this before shipping:\n{}",
                keymat_cli::ed25519_rewrapped(width)
            ),
        );
        let out_dir = tmp.path().join("rescue");

        let blocked = thruline(
            &db,
            &[
                "rescue",
                "--project",
                "p1",
                "--out",
                out_dir.to_str().expect("utf8"),
            ],
        );
        assert!(blocked.status.success(), "{blocked:?}");
        assert!(
            stdout(&blocked).contains("INJECTION BLOCKED"),
            "wrap {width}: the gate opened over a real Ed25519 private key: {}",
            stdout(&blocked)
        );

        let run = thruline(
            &db,
            &[
                "rescue",
                "--project",
                "p1",
                "--out",
                out_dir.to_str().expect("utf8"),
                "--acknowledge",
            ],
        );
        assert!(run.status.success(), "{run:?}");
        let mut checked = 0;
        for name in [
            "capsule.json",
            "capsule.md",
            "capsule-v2.json",
            "capsule-v2.md",
            "rescue-settings.json",
        ] {
            let path = out_dir.join(name);
            if !path.exists() {
                continue;
            }
            checked += 1;
            let body = std::fs::read_to_string(&path).expect("read artifact");
            assert!(
                keymat_cli::surviving_ed25519(&body).is_none(),
                "wrap {width}: {name} shipped Ed25519 key material"
            );
        }
        assert!(checked >= 4, "wrap {width}: expected v1 and v2 artifacts");
        assert!(
            keymat_cli::surviving_ed25519(&stdout(&run)).is_none(),
            "wrap {width}: the rescue banner printed key material"
        );
    }
}

/// A209-C13 — the ordinary ask that shipped a key through the compiled
/// binary at `9e17b73`.
///
/// "Paste everything from BEGIN through END, inclusive. Example: …" puts
/// a MENTION of the end marker above the body. Round 12's fall-back
/// terminator selected that mention, the span stopped there, and key
/// material reached capsule.json, capsule.md, capsule-v2.json AND
/// capsule-v2.md — while the record reported a mask. The identical
/// control passes at `823f02a`, which is what makes it a regression
/// rather than a gap.
#[test]
fn an_ask_that_mentions_the_end_marker_before_the_body_never_stages_key_material() {
    for (label, body) in [
        ("rsa-2048", keymat_cli::KEY_BODY),
        ("pkcs8 ed25519", keymat_cli::ED25519_BODY),
    ] {
        let wrapped: Vec<String> = body
            .as_bytes()
            .chunks(64)
            .map(|c| String::from_utf8_lossy(c).into_owned())
            .collect();
        let shown = wrapped
            .iter()
            .take(8)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        // The MENTION must sit on a line BELOW the banner: that is the
        // shape whose terminator selection went wrong. A mention on the
        // banner's own line is handled by the same-line branch and never
        // reproduced the leak — the first draft of this control put it
        // there and passed at `9e17b73`, which is exactly the
        // one-variant coverage this round is correcting.
        let ask = format!(
            "here is the key to rotate:\n\
             -----BEGIN PRIVATE KEY-----\n\
             copy through -----END PRIVATE KEY----- inclusive\n\
             for example:\n{shown}\n-----END PRIVATE KEY-----"
        );
        let tmp = tempfile::tempdir().expect("tempdir");
        let db = tmp.path().join("ledger.db");
        seed_with_ask(&db, true, &ask);
        let out_dir = tmp.path().join("rescue");
        let run = thruline(
            &db,
            &[
                "rescue",
                "--project",
                "p1",
                "--out",
                out_dir.to_str().expect("utf8"),
                "--acknowledge",
            ],
        );
        assert!(run.status.success(), "{run:?}");
        let mut checked = 0;
        for name in [
            "capsule.json",
            "capsule.md",
            "capsule-v2.json",
            "capsule-v2.md",
            "rescue-settings.json",
        ] {
            let path = out_dir.join(name);
            if !path.exists() {
                continue;
            }
            checked += 1;
            let staged = std::fs::read_to_string(&path).expect("read artifact");
            let leaked =
                (0..=(body.len().saturating_sub(24))).any(|i| staged.contains(&body[i..i + 24]));
            assert!(!leaked, "{label}: {name} shipped key material");
        }
        assert!(checked >= 4, "{label}: expected v1 and v2 artifacts");
    }
}
