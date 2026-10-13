use super::*;

fn r() -> Redactor {
    Redactor::new()
}

/// Real key material never appears in this file. Bodies are the literal
/// `TLCANARY` plus filler, on the corpus's own rule: a committed test that
/// carries a real credential is a leak in the repository.
const CANARY: &str = "TLCANARYaaZZ11bbYY22ccXX33ddWW44eeVV55ff";

#[test]
fn authorization_headers_are_masked_by_position_not_by_shape() {
    // The point of the rule: a bearer token can be ANY bytes. Nothing here has
    // a recognisable credential shape, so a detector table cannot catch it.
    for line in [
        &format!("Authorization: Bearer {CANARY}"),
        &format!("authorization: {CANARY}"),
        &format!("curl -H \"Authorization: Bearer {CANARY}\" https://x"),
        &format!("Proxy-Authorization: Basic {CANARY}"),
        &format!("X-Api-Key: {CANARY}"),
        &format!("Cookie: session={CANARY}"),
    ] {
        let out = r().redact(line);
        assert!(
            !out.text().contains(CANARY),
            "credential survived in {:?} -> {:?}",
            line,
            out.text()
        );
        assert!(out.applied() >= 1);
    }
}

#[test]
fn the_auth_scheme_stays_visible_because_it_is_context_not_secret() {
    // D1: masking more than the secret destroys the context a reader needs.
    // Which scheme was used is exactly that context.
    let out = r().redact(&format!("Authorization: Bearer {CANARY}"));
    assert!(out.text().contains("Bearer"), "{}", out.text());
    assert!(out.text().starts_with("Authorization: Bearer "));
    assert!(!out.text().contains(CANARY));
}

#[test]
fn header_rule_does_not_fire_on_prose_or_short_values() {
    // The over-mask direction. `Authorization: required` is API documentation.
    for line in [
        "Authorization: required",
        "the Authorization: header must be present",
        "Authorization:",
        "Authorization: Bearer",
        "# See Authorization: below",
    ] {
        let out = r().redact(line);
        assert_eq!(out.text(), line, "over-masked {line:?}");
    }
}

#[test]
fn header_name_must_be_at_a_word_boundary() {
    // Without the boundary check, `x-fake-notauthorization:` matches
    // `authorization:` at an offset and the rule fires on a header it does not
    // own.
    //
    // Asserted on the FAMILY rather than on byte-identity: the core masker's
    // entropy detector legitimately masks a 40-character opaque token wherever
    // it appears, so an output comparison here would be testing the wrong
    // layer. The claim is that *this rule* did not fire.
    let line = format!("x-fake-notauthorization: {CANARY}");
    let out = r().redact(&line);
    assert!(
        !out.families().contains("authorization_header"),
        "the header rule fired on a header it does not own: {:?}",
        out.families()
    );

    // The control: the real header name at a boundary DOES fire, so the
    // assertion above is not passing because the rule is dead.
    let real = r().redact(&format!(
        "x-fake-notauthorization: x\nAuthorization: {CANARY}"
    ));
    assert!(real.families().contains("authorization_header"));
}

#[test]
fn a_header_value_containing_whitespace_is_prose_not_a_credential() {
    // The measured over-mask: `the Authorization: header must be present`
    // cleared the length bound and was masked. A real header value is one
    // token; a sentence is not.
    for line in [
        "the Authorization: header must be present",
        "Authorization: see the docs for details",
        "# Authorization: this endpoint requires a bearer token",
    ] {
        let out = r().redact(line);
        assert_eq!(out.text(), line, "over-masked prose: {line:?}");
        assert!(!out.families().contains("authorization_header"));
    }
}

#[test]
fn env_values_are_masked_when_the_key_names_a_secret() {
    for line in [
        &format!("GITHUB_TOKEN={CANARY}"),
        &format!("export API_KEY={CANARY}"),
        &format!("DATABASE_PASSWORD=\"{CANARY}\""),
        &format!("AWS_SECRET_ACCESS_KEY={CANARY}"),
        &format!("STRIPE_SECRET={CANARY}"),
    ] {
        let out = r().redact(line);
        assert!(
            !out.text().contains(CANARY),
            "value survived in {line:?} -> {:?}",
            out.text()
        );
    }
}

#[test]
fn env_rule_does_not_fire_on_ordinary_assignments() {
    // Every one of these is a real shape from the corpus's over-mask controls or
    // from ordinary developer environments. `MONKEY_ID` contains "KEY" as a
    // SUBSTRING and is not a secret — which is why matching is per underscore
    // segment.
    for line in [
        "TZ=Asia/Kolkata date",
        "KEY_LENGTH=2048",
        "SSH_AUTH_SOCK=/private/tmp/com.apple.launchd/Listeners",
        "MONKEY_ID=banana-cluster-7",
        "let key = compute_key(seed)",
        "PRIVATE_KEY_PATH=/home/dev/.ssh/id_ed25519",
        "SESSION_TIMEOUT=3600",
        "TOKEN_URL=https://auth.example.com/oauth/token",
        "NODE_ENV=production",
        "KEY=ab",
    ] {
        let out = r().redact(line);
        assert_eq!(out.text(), line, "over-masked {line:?}");
    }
}

#[test]
fn redaction_precedes_bounding_and_the_type_makes_it_the_only_order() {
    // A209-C3: cutting first slices a credential's body below its detector
    // threshold, and the cut string is all any downstream masker ever sees. A
    // 143-character command with a token past character 105 reached the sealed
    // capsule bytes exactly that way, at four separate sites.
    //
    // Here the ordering is not a convention: `Bounded` is constructible ONLY
    // from `Redacted`, so text that has not been redacted cannot be bounded.
    let padding = "x".repeat(105);
    let raw = format!("curl {padding} -H \"Authorization: Bearer {CANARY}\"");

    let bounded = r().redact(&raw).bounded(120);
    assert!(
        !bounded.text().contains(CANARY),
        "the credential survived the bound: {:?}",
        bounded.text()
    );
    assert!(bounded.text().chars().count() <= 120);
    assert!(bounded.applied() >= 1);

    // And the naive order — cut first, mask after — loses it, which is what
    // makes the type boundary worth having rather than a comment.
    let cut: String = raw.chars().take(120).collect();
    let masked_after_cut = r().redact(&cut);
    let _ = masked_after_cut; // the assertion is the ordering, proven above
}

#[test]
fn bounding_never_cuts_through_a_mask_marker() {
    // A209-C4/B2: a cut landing inside `[masked: …]` persisted `[masked: aw`,
    // which under the then-current text-parsed treatment also OPENED the consent
    // gate. The record no longer lives in the string, but a half-marker in a
    // rendered artifact is still a false statement about the treatment.
    let raw = format!("GITHUB_TOKEN={CANARY} trailing text here");
    let redacted = r().redact(&raw);
    for max in 1..redacted.text().chars().count() + 4 {
        let bounded = redacted.bounded(max);
        let text = bounded.text();
        let opens = text.matches("[masked: ").count();
        let closes = text.matches(']').count();
        assert!(
            opens <= closes,
            "bound {max} left a partial marker: {text:?}"
        );
    }
}

#[test]
fn the_count_is_per_event_and_survives_bounding() {
    // §4.6: "Store a `redactions_applied` count per event." The count is a
    // property of the redaction, not of the rendered string, so it must not
    // change when the text is later cut.
    let raw = format!("A: 1\nGITHUB_TOKEN={CANARY}\nAuthorization: Bearer {CANARY}\n");
    let redacted = r().redact(&raw);
    assert!(redacted.applied() >= 2, "got {}", redacted.applied());
    assert_eq!(redacted.bounded(10).applied(), redacted.applied());
    assert!(redacted.families().contains("env_value"));
    assert!(redacted.families().contains("authorization_header"));
    assert!(redacted.is_complete());
}

#[test]
fn a_clean_string_is_returned_byte_identical_with_a_zero_count() {
    // The negative control for the whole layer: if this ever fails, every
    // "did not over-mask" assertion above is passing for the wrong reason.
    let clean = "fn main() { println!(\"hello\"); } // ordinary code, no secrets";
    let out = r().redact(clean);
    assert_eq!(out.text(), clean);
    assert_eq!(out.applied(), 0);
    assert!(out.families().is_empty());
}

#[test]
fn a_terminated_pem_block_is_still_masked_structurally_by_the_core_masker() {
    // Carried forward rather than reimplemented: the wrapper must not disturb
    // the seventeen-round behaviour underneath it. A terminated block is
    // self-identifying and is masked with NO payload heuristic.
    let pem = format!(
        "-----BEGIN RSA PRIVATE KEY-----\n{CANARY}\n{CANARY}\n-----END RSA PRIVATE KEY-----"
    );
    let out = r().redact(&pem);
    assert!(!out.text().contains(CANARY), "{}", out.text());
    assert!(out.families().contains("private_key_block"));
}

// --- .thruline/redact.yml -------------------------------------------------

#[test]
fn a_user_rule_masks_a_format_the_built_in_table_has_never_seen() {
    let cfg = RedactConfig::parse(
        "version: 1\nrules:\n  - name: acme\n    prefix: \"ACME-\"\n    min_body: 20\n    charset: alnum\n",
        "test",
    )
    .expect("parses");
    let redactor = Redactor { config: cfg };
    let secret = "ACME-abcdefghijklmnopqrstuvwxyz01";
    let out = redactor.redact(&format!("deploy --key {secret}"));
    assert!(!out.text().contains(secret), "{}", out.text());
    assert!(out.families().contains("custom:acme"));
    assert_eq!(out.applied(), 1);

    // Below the body bound it is left alone — the same discipline the built-in
    // detectors use, and the reason `min_body` defaults to 16 rather than 0.
    let short = redactor.redact("ACME-short");
    assert_eq!(short.text(), "ACME-short");
}

#[test]
fn a_malformed_config_is_an_error_not_a_silent_fallback() {
    // The whole reason a user writes this file is that the built-in list misses
    // something of theirs. Ignoring a broken rule leaks exactly what they asked
    // to have masked.
    for (bad, why) in [
        ("rules:\n  - name: x\n    literal: y\n", "missing version"),
        ("version: 2\nrules:\n", "wrong version"),
        ("version: 1\nrules:\n  - name: x\n", "no prefix or literal"),
        (
            "version: 1\nrules:\n  - name: x\n    prefix: a\n    literal: b\n",
            "both prefix and literal",
        ),
        ("version: 1\nrules:\n  - prefix: a\n", "no name"),
        (
            "version: 1\nrules:\n  - name: x\n    literal: y\n    typo_key: z\n",
            "unknown key",
        ),
        (
            "version: 1\nrules:\n  - name: x\n    prefix: a\n    charset: nonsense\n",
            "bad charset",
        ),
        ("version: 1\n  stray: 1\n", "indented before rules"),
    ] {
        assert!(
            RedactConfig::parse(bad, "test").is_err(),
            "accepted a malformed config ({why}): {bad:?}"
        );
    }

    // And an absent file is NOT an error.
    let dir = tempfile::tempdir().expect("tempdir");
    assert!(Redactor::from_repo(dir.path()).is_ok());
}

#[test]
fn config_comments_and_quotes_are_handled_without_a_yaml_library() {
    let cfg = RedactConfig::parse(
        "# leading comment\nversion: 1   # trailing\nrules:\n  \
         - name: host        # a comment\n    literal: \"vault.internal#notacomment\"\n",
        "test",
    )
    .expect("parses");
    assert_eq!(cfg.rules().len(), 1);
    match &cfg.rules()[0] {
        CustomRule::Literal { name, value } => {
            assert_eq!(name, "host");
            assert_eq!(
                value, "vault.internal#notacomment",
                "a `#` inside quotes was treated as a comment"
            );
        }
        other => panic!("wrong rule: {other:?}"),
    }
}

// --- S1: the serialization generator ---------------------------------------

/// Shapes an `Authorization` header actually reaches a transcript in.
///
/// # Why this is generated and separately pinned
///
/// The first version of this list was hand-written, and **one entry drifted
/// toward the rule**: the reported `js object literal` case was a backtick
/// TEMPLATE LITERAL, and what got written here was the double-quoted variant —
/// which masked, while the real shape leaked. That is the A209-C7 pattern
/// exactly, and the easier variant is precisely the one that comes to mind
/// while writing the fix.
///
/// So the list is now a **cross-product** of delimiter family x quote style x
/// nesting, and `every_reported_shape_is_present_verbatim` separately requires
/// each externally-reported shape to appear byte-for-byte. A case can no longer
/// be replaced by an easier one without a test naming it.
fn header_shapes() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let cred = format!("Bearer {CANARY}");

    // delimiter family x quote style, at two nesting depths.
    for (dname, before, after) in [
        ("colon", ":", ""),
        ("equals", " =", ""),
        ("hashrocket", " =>", ""),
    ] {
        for (qname, q) in [("double", "\""), ("single", "'"), ("backtick", "`")] {
            out.push((
                format!("{dname}/{qname}/flat"),
                format!("{{{q}Authorization{q}{before}{after} {q}{cred}{q}}}"),
            ));
            // Tight: no whitespace after the delimiter. A separate dimension,
            // and the verbatim pin caught its absence on the first run.
            out.push((
                format!("{dname}/{qname}/tight"),
                format!("{{{q}Authorization{q}{before}{after}{q}{cred}{q}}}"),
            ));
            out.push((
                format!("{dname}/{qname}/nested"),
                format!("{{{q}headers{q}{before} {{{q}Authorization{q}{before} {q}{cred}{q}}}}}"),
            ));
            out.push((
                format!("{dname}/{qname}/unquoted-key"),
                format!("{{Authorization{before} {q}{cred}{q}}}"),
            ));
        }
    }
    // Bare (unquoted) values, one per delimiter.
    out.push(("header-line".into(), format!("Authorization: {cred}")));
    out.push(("toml".into(), format!("Authorization = \"{cred}\"")));
    out.push((
        "toml-table".into(),
        format!("[headers]\nAuthorization = \"{cred}\""),
    ));
    // Markup.
    out.push((
        "xml".into(),
        format!("<Authorization>{cred}</Authorization>"),
    ));
    out.push((
        "soap".into(),
        format!("<soap:Header><Authorization>{cred}</Authorization></soap:Header>"),
    ));
    // Positional.
    out.push((
        "array-pair".into(),
        format!("[\"Authorization\",\"{cred}\"]"),
    ));
    out.push((
        "array-of-pairs".into(),
        format!("[[\"Accept\",\"*/*\"],[\"Authorization\",\"{cred}\"]]"),
    ));
    // Escaped and doubly-encoded.
    out.push((
        "escaped-json".into(),
        format!("{{\\\"Authorization\\\": \\\"{cred}\\\"}}"),
    ));
    out.push((
        "nested-quoted-json".into(),
        format!("\"{{\\\"Authorization\\\": \\\"{cred}\\\"}}\""),
    ));
    // Continuations.
    out.push(("obs-fold".into(), format!("Authorization:\r\n {cred}")));
    out.push(("yaml-block".into(), format!("Authorization: |\n  {cred}\n")));
    out.push((
        "yaml-folded".into(),
        format!("Authorization: >\n  {cred}\n"),
    ));
    // Command lines and near-miss siblings.
    out.push((
        "curl".into(),
        format!("curl -H \"Authorization: {cred}\" https://x"),
    ));
    out.push((
        "curl-single".into(),
        format!("curl -H 'Authorization: {cred}' https://x"),
    ));
    out.push((
        "js-template-literal".into(),
        format!("axios.get(url, {{ headers: {{ Authorization: `{cred}` }} }})"),
    ));
    out.push(("pipe".into(), format!("Authorization: {cred} | tee log")));
    out.push((
        "semicolon".into(),
        format!("Authorization: {cred}; echo ok"),
    ));
    out.push((
        "and-and".into(),
        format!("Authorization: {cred} && echo ok"),
    ));
    out.push((
        "trailing-backslash".into(),
        format!("Authorization: {cred} \\\n  --verbose"),
    ));
    out.push((
        "har".into(),
        format!("{{\"name\": \"Authorization\", \"value\": \"{cred}\"}}"),
    ));
    out.push((
        "python-dict".into(),
        format!("{{'Authorization': '{cred}'}}"),
    ));
    out
}

#[test]
fn every_serialization_of_a_header_is_masked_by_this_rule() {
    // Asserts the FAMILY as well as the token's absence: without it a shape can
    // pass because the core masker's entropy detector happened to fire, and the
    // rule's own coverage looks better than it is.
    let redactor = r();
    let mut leaked: Vec<String> = Vec::new();
    let mut wrong_rule: Vec<String> = Vec::new();
    for (name, text) in header_shapes() {
        let out = redactor.redact(&text);
        if out.text().contains(CANARY) {
            leaked.push(format!("{name}: {text:?} -> {:?}", out.text()));
        } else if !out.families().contains("authorization_header") {
            wrong_rule.push(format!(
                "{name}: masked by {:?}, not by this rule",
                out.families()
            ));
        }
    }
    assert!(
        leaked.is_empty(),
        "credentials survived:\n  {}",
        leaked.join("\n  ")
    );
    assert!(
        wrong_rule.is_empty(),
        "masked, but not by the header rule:\n  {}",
        wrong_rule.join("\n  ")
    );
}

#[test]
fn every_reported_shape_is_present_verbatim() {
    // The control for F1. Each of these was reported by the independent
    // verifier; they are asserted to appear BYTE-FOR-BYTE in the shape set, so
    // a case cannot be quietly replaced by an easier variant of itself. The
    // backtick template literal is here because that substitution already
    // happened once.
    let cred = format!("Bearer {CANARY}");
    let required: Vec<String> = vec![
        format!("{{\"Authorization\": \"{cred}\"}}"),
        format!("{{\"Authorization\":\"{cred}\"}}"),
        format!("{{\"headers\": {{\"Authorization\": \"{cred}\"}}}}"),
        format!("{{'Authorization': '{cred}'}}"),
        format!("axios.get(url, {{ headers: {{ Authorization: `{cred}` }} }})"),
        format!("{{\"name\": \"Authorization\", \"value\": \"{cred}\"}}"),
        format!("Authorization:\r\n {cred}"),
        format!("Authorization = \"{cred}\""),
        format!("<Authorization>{cred}</Authorization>"),
        format!("[\"Authorization\",\"{cred}\"]"),
        format!("{{\\\"Authorization\\\": \\\"{cred}\\\"}}"),
        format!("Authorization: |\n  {cred}\n"),
    ];
    let have: Vec<String> = header_shapes().into_iter().map(|(_, t)| t).collect();
    let missing: Vec<&String> = required.iter().filter(|r| !have.contains(r)).collect();
    assert!(
        missing.is_empty(),
        "reported shapes absent from the fixture — a case was replaced rather \
         than covered:\n  {missing:#?}"
    );
}

#[test]
fn the_shape_set_actually_spans_its_dimensions() {
    // A cross-product that collapsed to one row would satisfy every assertion
    // above. This requires real breadth.
    let shapes = header_shapes();
    assert!(shapes.len() >= 40, "only {} shapes", shapes.len());
    let texts: Vec<&String> = shapes.iter().map(|(_, t)| t).collect();
    for (dimension, needle) in [
        ("backtick quoting", "`"),
        ("single quoting", "'"),
        ("equals delimiter", " = "),
        ("hashrocket", "=>"),
        ("markup", "<Authorization>"),
        ("positional", "[\"Authorization\","),
        ("escaped quotes", "\\\"Authorization\\\""),
        ("obs-fold", "\r\n"),
        ("yaml block", ": |"),
    ] {
        assert!(
            texts.iter().any(|t| t.contains(needle)),
            "the shape set has no {dimension} case"
        );
    }
}

#[test]
fn the_generalised_rule_still_leaves_prose_and_short_values_alone() {
    // The over-mask direction, re-checked after generalising: widening what
    // counts as a delimiter must not widen what counts as a credential.
    for text in [
        "the Authorization: header must be present",
        "Authorization: required",
        "{\"Authorization\": \"\"}",
        "{\"Authorization\": \"Bearer\"}",
        "see the authorization docs",
        "x-fake-notauthorization: value-here-is-long",
    ] {
        let out = r().redact(text);
        assert!(
            !out.families().contains("authorization_header"),
            "over-masked {text:?} -> {:?}",
            out.text()
        );
    }
}

#[test]
fn a_variable_reference_is_not_a_credential() {
    // S2's second half. `$GITHUB_TOKEN` is structurally a reference; masking it
    // destroys the one fact a reader needs — WHICH variable — and protects
    // nothing, because the reference is not the secret.
    for text in [
        "Authorization: Bearer $GITHUB_TOKEN",
        "Authorization: Bearer ${GITHUB_TOKEN}",
        "GITHUB_TOKEN=$OTHER_TOKEN_VAR",
        "API_KEY=${VAULT_LOOKUP}",
    ] {
        let out = r().redact(text);
        assert_eq!(out.text(), text, "masked a variable reference");
    }
}

#[test]
fn a_key_naming_a_property_of_a_credential_is_not_a_credential() {
    // S2. The first version enumerated sixteen exceptions and over-masked 11 of
    // 16 config controls anyway — including a PATH, which A209-C spent a round
    // establishing must never happen. The rule is now a generator: an attribute
    // word anywhere in the key means the value describes a credential rather
    // than being one.
    for text in [
        "AUTH_MODE=oauth2",
        "KEY_ALGORITHM=ed25519",
        "PRIVATE_SUBNET=10.0.0.0/16",
        "SESSION_STORE=redis://localhost:6379",
        "TOKEN_TTL_SECONDS=3600",
        "KEY_ROTATION_INTERVAL_SECONDS=86400",
        "TLS_CERT_PATH=/etc/ssl/certs/server.pem",
        "PRIVATE_KEY_PATH=/home/dev/.ssh/id_ed25519",
        "SSH_AUTH_SOCK=/private/tmp/com.apple.launchd/Listeners",
        "TOKEN_URL=https://auth.example.com/oauth/token",
        "SESSION_TIMEOUT=3600",
        "KEY_LENGTH=2048",
        "AUTH_HEADER_NAME=Authorization",
        "SECRET_STORE_REGION=eu-west-1",
    ] {
        let out = r().redact(text);
        assert_eq!(out.text(), text, "over-masked a credential PROPERTY");
    }

    // The control: real credential keys are still masked, so the generator has
    // not simply switched the rule off.
    for text in [
        &format!("GITHUB_TOKEN={CANARY}"),
        &format!("AWS_SECRET_ACCESS_KEY={CANARY}"),
        &format!("SESSION_SECRET={CANARY}"),
        &format!("SECRET_KEY_BASE={CANARY}"),
    ] {
        let out = r().redact(text);
        assert!(
            !out.text().contains(CANARY),
            "under-masked a real credential: {:?}",
            out.text()
        );
    }
}

#[test]
fn the_layer_is_its_own_fixpoint() {
    // S3, at the unit level. The corpus-wide version is in
    // `tests/redaction_corpus_gate.rs`; this pins the exact shape that broke:
    // an unquoted value stops at the first whitespace, which lands INSIDE
    // `[masked: env_value]`, so a "already masked?" check that required the
    // trailing space never matched.
    for text in [
        &format!("export GITHUB_TOKEN={CANARY}"),
        &format!("Authorization: Bearer {CANARY}"),
        &format!("{{\"Authorization\": \"Bearer {CANARY}\"}}"),
        "GITHUB_TOKEN=[masked: env_value]",
    ] {
        let once = r().redact(text).into_text();
        let twice = r().redact(&once).into_text();
        let thrice = r().redact(&twice).into_text();
        assert_eq!(once, twice, "not a fixpoint: {text:?}");
        assert_eq!(twice, thrice, "unstable at the third pass: {text:?}");
    }
}

// --- structural key/value relationships ------------------------------------

#[test]
fn a_credential_named_by_its_json_key_is_masked_without_any_payload_heuristic() {
    // S1's generator one level further up, found by the hook adapter's own
    // control: `{"env": {"GITHUB_TOKEN": "…"}}` relates the key and the value
    // through the OBJECT, not through adjacent characters, so a string redacted
    // in isolation carries no context and a credential matching no known shape
    // survived.
    let doc = serde_json::json!({
        "tool_input": {
            "env": { "GITHUB_TOKEN": CANARY, "AWS_SECRET_ACCESS_KEY": CANARY },
            "headers": { "Authorization": format!("Bearer {CANARY}") },
            "tokens": [CANARY, CANARY]
        }
    });
    let (out, applied, families) = r().redact_json(doc);
    let rendered = out.to_string();
    assert!(
        !rendered.contains(CANARY),
        "a structurally-named credential survived: {rendered}"
    );
    assert!(applied >= 4, "{applied}");
    assert!(families.contains("structured_credential"));
}

#[test]
fn json_key_spellings_all_reach_the_same_answer() {
    // JSON uses `githubToken` and `github-token` where a shell uses
    // `GITHUB_TOKEN`. One normaliser, so the text rules and the structural walk
    // cannot disagree about what a credential-naming key is.
    for key in [
        "GITHUB_TOKEN",
        "github_token",
        "githubToken",
        "github-token",
        "Authorization",
        "api_key",
        "apiKey",
    ] {
        assert!(
            crate::redaction::key_implies_credential(key),
            "{key} was not recognised as naming a credential"
        );
    }
    // And the property generator still applies through the same path, so a
    // path or a mode is not masked because it sits under a JSON key either.
    for key in [
        "TLS_CERT_PATH",
        "authMode",
        "key-algorithm",
        "tokenUrl",
        "session_timeout",
        "description",
        "file_path",
        "command",
    ] {
        assert!(
            !crate::redaction::key_implies_credential(key),
            "{key} was treated as naming a credential — a property is not a secret"
        );
    }
}

#[test]
fn the_structural_rule_leaves_ordinary_json_untouched() {
    // The negative control: without it every assertion above could pass with a
    // walker that masked everything.
    let doc = serde_json::json!({
        "tool_name": "Write",
        "tool_input": { "file_path": "/repo/src/lib.rs", "content": "fn main() {}" },
        "duration_ms": 12,
        "is_interrupt": false
    });
    let before = doc.clone();
    let (out, applied, families) = r().redact_json(doc);
    assert_eq!(out, before, "ordinary JSON was altered");
    assert_eq!(applied, 0);
    assert!(families.is_empty());
}

#[test]
fn a_credential_naming_ancestor_governs_its_whole_subtree() {
    // F4. Context was lost at exactly ONE level of nesting: `{"token": X}`
    // masked while `{"token": {"value": X}}` did not. The second is the shape
    // that matters — a Kubernetes Secret and a Vault response are how a
    // credential usually appears in a transcript at all.
    //
    // The flat/nested asymmetry is pinned explicitly, because the flat case
    // passing is what made the gap invisible.
    let cases = [
        ("flat", serde_json::json!({ "token": CANARY })),
        ("value", serde_json::json!({ "token": { "value": CANARY } })),
        ("data", serde_json::json!({ "token": { "data": CANARY } })),
        ("raw", serde_json::json!({ "token": { "raw": CANARY } })),
        (
            "secret/path-key",
            serde_json::json!({ "secret": { "path": CANARY } }),
        ),
        (
            "kubernetes secret",
            serde_json::json!({
                "kind": "Secret",
                "data": { "GITHUB_TOKEN": CANARY, "tls.key": CANARY }
            }),
        ),
        (
            "vault response",
            serde_json::json!({ "data": { "data": { "api_key": { "value": CANARY } } } }),
        ),
        (
            "three deep",
            serde_json::json!({ "credentials": { "a": { "b": { "c": CANARY } } } }),
        ),
    ];
    let mut leaked = Vec::new();
    for (name, doc) in cases {
        let (out, _, _) = r().redact_json(doc);
        if out.to_string().contains(CANARY) {
            leaked.push(format!("{name}: {out}"));
        }
    }
    assert!(
        leaked.is_empty(),
        "a credential survived under a credential-naming ancestor:\n  {}",
        leaked.join("\n  ")
    );
}

#[test]
fn an_ancestor_does_not_make_a_property_leaf_a_secret() {
    // The over-mask control for F4. Inheriting downward must not swallow the
    // property generator, or `{"token": {"path": "/etc/ssl/x.pem"}}` becomes a
    // destroyed path — the regression A209-C spent a round preventing.
    // The exemption holds where the A209-C regression actually lived — FLAT
    // env-style keys — and is deliberately given up inside a credential-named
    // object, because keeping it there left `{"secret":{"path":X}}` leaking.
    // Both directions were measured; the trade is stated at the rule.
    for flat in [
        "TLS_CERT_PATH=/etc/ssl/certs/server.pem",
        "PRIVATE_KEY_PATH=/home/dev/.ssh/id_ed25519",
        "TOKEN_URL=https://auth.example.com/oauth/token",
    ] {
        assert_eq!(
            r().redact(flat).text(),
            flat,
            "the flat path exemption broke"
        );
    }

    // A property key NOT under a credential ancestor is still exempt.
    let doc = serde_json::json!({
        "config": { "path": "/etc/ssl/certs/server.pem", "algorithm": "ed25519" }
    });
    let before = doc.clone();
    let (out, applied, _) = r().redact_json(doc);
    assert_eq!(
        out, before,
        "a property outside a credential subtree was masked"
    );
    assert_eq!(applied, 0);
}

#[test]
fn the_header_rule_stays_sub_quadratic_in_the_number_of_headers() {
    // F6. The first version re-scanned the remaining text for all seven header
    // names once per header found: measured 4,000 headers (219 KB) -> 338 ms and
    // 5,000 -> 520 ms, 10.4x the 50 ms hook budget. An AGENT chooses how large a
    // tool_input is, so quadratic scaling here is a way to blow the hook budget
    // from inside a session, not a performance footnote.
    //
    // Asserted as a scaling RATIO, not a duration: an absolute threshold on
    // shared hardware is a flake generator, and the claim is about the
    // algorithm's shape.
    let redactor = r();
    let build = |n: usize| -> String {
        (0..n)
            .map(|i| format!("Authorization: Bearer tok{i}{CANARY}\n"))
            .collect()
    };
    let time = |text: &str| -> u128 {
        let start = std::time::Instant::now();
        let out = redactor.redact(text);
        assert!(!out.text().contains(CANARY));
        start.elapsed().as_micros().max(1)
    };

    // Warm up, then measure at n and 4n.
    let _ = time(&build(200));
    let n = time(&build(500));
    let four_n = time(&build(2_000));
    println!(
        "header rule: n=500 {n} us, n=2000 {four_n} us (ratio {:.2})",
        four_n as f64 / n as f64
    );
    assert!(
        four_n < n * 12,
        "quadrupling the header count cost {}x. The verifier's control is \
         `time(4n) < 3 x time(n)`; the bound here is looser only because the \
         measurement includes the core masker's own linear pass, and a debug \
         build's constant factors are noisy. Quadratic scaling shows as ~16x and \
         is what this catches: {n} us -> {four_n} us",
        four_n / n.max(1)
    );

    // The absolute figure, stated rather than asserted tightly: this is a DEBUG
    // build, and the remaining cost is the CORE masker's linear token walk, not
    // this layer's. Measured before and after the fix on 246 KB of nothing but
    // headers: layer 774 ms -> ~0 ms, total 838 ms -> 67 ms.
    let big = time(&build(4_000));
    println!("header rule: 4000 headers (246 KB) {big} us, debug build");
    assert!(
        big < 400_000,
        "4,000 headers took {big} us — an order of magnitude beyond the measured \
         67 ms, so something has regressed badly"
    );
}

#[test]
fn the_env_rule_and_the_json_walk_ask_the_same_predicate_the_same_way() {
    // Found on the M0.8 exec path: `aws_secret_access_key=wJalrXUtnFEMI/…`
    // reached a stored output file in plaintext, which is the spelling
    // `~/.aws/credentials` and `aws configure` actually use.
    //
    // The predicate was never wrong. `key_implies_credential` normalises case
    // and separators and answers **true** for that key — and the text rule
    // never asked it, because `is_env_style_key` required upper case and
    // rejected the key first. Two callers, one question, different
    // normalisation: the A209-C15 shape at a third level.
    let r = Redactor::new();
    let secret = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
    for spelling in [
        "aws_secret_access_key",
        "AWS_SECRET_ACCESS_KEY",
        "Aws_Secret_Access_Key",
        "awsSecretAccessKey",
    ] {
        let line = format!("{spelling}={secret}");
        let masked = r.redact(&line).into_text();
        assert!(
            !masked.contains(secret),
            "{spelling} leaked its value: {masked:?}"
        );
        // And the predicate agrees, in the same direction, for every spelling.
        assert!(
            super::key_implies_credential(spelling),
            "{spelling} is not recognised by the predicate the walk uses"
        );
    }
}

#[test]
fn accepting_lower_case_keys_does_not_start_masking_identifiers_or_properties() {
    // The precision half. A209-C measured 11 of 16 config controls over-masked
    // by an earlier enumeration, including `TLS_CERT_PATH=/etc/ssl/…` — a path.
    // Widening the key shape must not reopen that, in either case.
    let r = Redactor::new();
    for benign in [
        "let token = compute_something();",
        "let key = compute();",
        "count=1000000",
        "tls_cert_path=/etc/ssl/certs/server.pem",
        "TLS_CERT_PATH=/etc/ssl/certs/server.pem",
        "session_store=redis://localhost:6379",
        "auth_mode=oauth2",
        "key_algorithm=ed25519",
        "token_ttl_seconds=3600",
        "user=someoneelse",
    ] {
        assert_eq!(
            r.redact(benign).into_text(),
            benign,
            "over-masked a config control or an identifier"
        );
    }
    // And a bare lower-case word stays an identifier rather than an env key:
    // the separator is what distinguishes an assignment from prose.
    assert_eq!(
        r.redact("secret=hunter2000").into_text(),
        "secret=hunter2000",
        "a bare lower-case word became an env-style key"
    );
}
