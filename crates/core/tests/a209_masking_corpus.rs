//! A209-C — the adversarial masking corpus, as a standing regression gate.
//!
//! The 173-case corpus that established Stage 6's masking numbers was
//! authored by an agent that did not write the fix (MANDATE-v4 §32: the
//! implementation agent may not be the only test author or oracle). It
//! lived only at a scratchpad path, so "MUST MASK 76/76 · MUST NOT MASK
//! 75/77" was a one-time measurement nothing would notice ceasing to be
//! true — while `redact.rs` told future reviewers it was "pinned by
//! `c3_redaction_corpus.rs`", a file that has not changed since A178 and
//! contains none of the specimens. That false pointer is why this file
//! exists.
//!
//! **Specimens are shape-derived, never copied**, on `c3_redaction_corpus`'s
//! own rule: every secret body is the literal `TLCANARY`, and where the
//! live corpus contributed a false-positive specimen only its *shape* is
//! reproduced. No real credential and no private path is committed here.
//!
//! Two cases are pinned as **named expected exceptions** rather than
//! silently dropped: they fail before and after the A209 repair, they are
//! enumerated in `redact.rs`'s measured-misses section, and this file
//! asserts they still fail — so curing them later breaks this test loudly
//! instead of passing unnoticed.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use thruline_core::redact::MaskUse;

use thruline_core::redact::{
    mask, resembles_known_credential, TruncationGuard, CONDITIONAL_TRUNCATIONS, MASK_CALL_SITES,
    NON_RENDERING_TRUNCATIONS, TRUNCATION_SITES,
};

/* ── shape-derived canaries ─────────────────────────────────────────── */

const BODY: &str = "TLCANARYaaZZ11bbYY22ccXX33ddWW44e";
/// A real PEM payload line: RFC 7468 wraps at 64 characters, and the body
/// gate requires at least 40 — a 33-character canary is not key material.
const PEM_BODY: &str = "MIIEowIBAAKCAQEATLCANARYaaZZ11bbYY22ccXX33ddWW44eeVV55ffUU66gg77";
fn gh(letter: char) -> String {
    format!("gh{letter}_{BODY}")
}
fn ghpat() -> String {
    format!("github_pat_TLCANARY11AAAAAAAAAA_{BODY}")
}
const AWS: &str = "AKIATLCANARY7EXAMPLE";
const ASIA: &str = "ASIATLCANARY7EXAMPLE";
fn oai() -> String {
    format!("sk-{BODY}")
}
fn ant() -> String {
    format!("sk-ant-api03-{BODY}")
}
fn slack(kind: &str) -> String {
    format!("{kind}-111111111111-222222222222-TLCANARYaaZZ11bbYY22cc")
}
const GOOG: &str = "AIzaTLCANARYaaZZ11bbYY22ccXX33ddWW4";
const JWT: &str = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJUTENBTkFSWSJ9.TLCANARYsig";

/// Every case: (id, input, expected detector name).
fn must_mask() -> Vec<(&'static str, String, &'static str)> {
    let mut v: Vec<(&'static str, String, &'static str)> = vec![
        ("M01", gh('p'), "github"),
        ("M02", gh('o'), "github"),
        ("M03", gh('s'), "github"),
        // The two formats that were absent from the table entirely until
        // A209-C round 3 — user-to-server and refresh tokens.
        ("M03u", gh('u'), "github"),
        ("M03r", gh('r'), "github"),
        ("M04", ghpat(), "github"),
        ("M05", AWS.to_string(), "aws"),
        ("M06", ASIA.to_string(), "aws"),
        ("M07", oai(), "openai"),
        ("M08", format!("sk-proj-{BODY}"), "openai"),
        ("M09", ant(), "anthropic"),
        ("M10", slack("xoxb"), "slack"),
        ("M11", slack("xoxp"), "slack"),
        (
            "M12",
            "xapp-1-A11111111-222222222222-TLCANARYaaZZ11bbYY22cc".to_string(),
            "slack",
        ),
        ("M13", GOOG.to_string(), "google"),
        ("M14", JWT.to_string(), "jwt"),
        // A209-C6/B8-3: a banner is a key only with key material behind
        // it. The bare forms moved to the must-NOT-mask half (N84-N86).
        (
            "M16",
            format!("-----BEGIN RSA PRIVATE KEY-----\n{PEM_BODY}\n-----END RSA PRIVATE KEY-----"),
            "private_key_block",
        ),
        (
            "M72",
            format!("-----BEGIN PGP MESSAGE-----\n{PEM_BODY}\n-----END PGP MESSAGE-----"),
            "private_key_block",
        ),
        // The RFC 4880 armor label round 6 rejected (A209-C6/B8-1) — the
        // most common way a private key appears in a transcript.
        (
            "M72b",
            format!(
                "-----BEGIN PGP PRIVATE KEY BLOCK-----\n{PEM_BODY}\n-----END PGP PRIVATE KEY BLOCK-----"
            ),
            "private_key_block",
        ),
    ];
    // 1b — every prefix behind `_`, the original A209 defect class.
    for (id, prefix, name) in [
        ("M17", gh('p'), "github"),
        ("M18", gh('o'), "github"),
        ("M19", gh('s'), "github"),
        ("M19u", gh('u'), "github"),
        ("M19r", gh('r'), "github"),
        ("M20", ghpat(), "github"),
        ("M23", oai(), "openai"),
        ("M24", ant(), "anthropic"),
        ("M28", JWT.to_string(), "jwt"),
    ] {
        v.push((id, format!("secrets_{prefix}"), name));
    }
    v.push(("M21", format!("key_{AWS}"), "aws"));
    v.push(("M22", format!("key_{ASIA}"), "aws"));
    v.push(("M25", format!("bot_{}", slack("xoxb")), "slack"));
    v.push(("M27", format!("maps_{GOOG}"), "google"));
    // 1c — hostile positions.
    let g = gh('p');
    for (id, input, name) in [
        ("M29", format!("backup.{g}.bak"), "github"),
        ("M30", format!("prefix-{g}"), "github"),
        ("M31", format!("/tmp/repo/secrets_{g}.txt"), "github"),
        ("M32", format!("/tmp/key_{AWS}.txt"), "aws"),
        ("M33", format!("src/{AWS}.txt"), "aws"),
        ("M34", format!("{g}_backup"), "github"),
        ("M35", format!("a/{g}/b"), "github"),
        ("M36", format!("dir/sub/{g}"), "github"),
        ("M37", format!("\"{g}\""), "github"),
        ("M38", format!("'{g}'"), "github"),
        ("M39", format!("{{\"token\": \"{g}\"}}"), "github"),
        ("M40", format!("the key `{g}` was used"), "github"),
        ("M41", format!("({g}),"), "github"),
        ("M42", format!("used {g}."), "github"),
        ("M43", format!("[see]({g})"), "github"),
        ("M44", format!("{AWS}:{g}"), "aws"),
        ("M45", format!("{g}/{AWS}"), "aws"),
        ("M46", format!("{AWS} and {g}"), "aws"),
        (
            "M47",
            format!("git push https://{g}@github.com/x/y"),
            "github",
        ),
        ("M48", format!("//{g}"), "github"),
        (
            "M49",
            format!("https://user:{}@api.example.com/v1", oai()),
            "openai",
        ),
        ("M50", format!("GITHUB_TOKEN={g}"), "github"),
        ("M51", format!("AWS_ACCESS_KEY_ID={AWS}"), "aws"),
        (
            "M52",
            format!("Authorization: Bearer {}", ant()),
            "anthropic",
        ),
        ("M53", format!("--token {g}"), "github"),
        ("M54", format!("export GH_TOKEN='{g}'"), "github"),
        ("M55", format!("cookie=sid.{JWT}"), "jwt"),
        (
            "M56",
            format!("-Users-me-repo--worktree-secrets_{g}"),
            "github",
        ),
        ("M57", format!("Read(/etc/{g}.pem)"), "github"),
        ("M58", format!("~/.aws/{AWS}"), "aws"),
        ("M59", format!("v2.{g}"), "github"),
        ("M60", g.clone(), "github"),
    ] {
        v.push((id, input, name));
    }
    // 1d — the shapes a Recovery Capsule actually renders.
    for (id, input, name) in [
        (
            "M61",
            format!("3 edits after the last commit \u{2014} unresolved \u{b7} /r/secrets_{g}.txt"),
            "github",
        ),
        (
            "M62",
            format!(
                "17 edits after the last commit \u{2014} unresolved \u{b7} \
                 apps/web/src/lib/task-board.ts, config/key_{g}.env"
            ),
            "github",
        ),
        (
            "M63",
            format!(
                "1 edits after the last commit \u{2014} unresolved \u{b7} config/key_{AWS}.env"
            ),
            "aws",
        ),
        (
            "M64",
            format!(
                "`git push https://{g}@github.com/x/y` \u{b7} failed \u{b7} replay class unsafe"
            ),
            "github",
        ),
        (
            "M65",
            format!("transcript /Users/x/notes/token_{g}.md:42"),
            "github",
        ),
        ("M66", format!("\u{201c}rotate {g}\u{201d}"), "github"),
        (
            "M67",
            format!("filesystem /Users/x/repo/{AWS}.txt (sha256 e3b0c442)"),
            "aws",
        ),
        (
            "M73",
            format!("12 edits \u{b7} config/key_{AWS}.env"),
            "aws",
        ),
        (
            "M74",
            format!("Bash(git remote add o https://{g}@github.com/a/b)"),
            "github",
        ),
        ("M75", format!("EXPO_PUBLIC_MAPS={GOOG}"), "google"),
        ("M71", format!("{}@example.com", ant()), "anthropic"),
    ] {
        v.push((id, input, name));
    }
    v.push((
        "M68",
        "git -c user.email=\"someone@example.com\" commit".to_string(),
        "email",
    ));
    v.push((
        "M69",
        "supabase functions deploy fn --project-ref qmfxkvtjwzbnlcdhr".to_string(),
        "flag_value",
    ));
    v.push((
        "M70",
        "TOKEN=f3Kq9ZbW2xLpR7vN4tJc8mHd5sYgA1eU6oI0".to_string(),
        "high_entropy",
    ));
    v.push(("M76", "task-board@example.com".to_string(), "email"));
    v
}

/// Shape-derived: real-corpus false positives reproduced as shapes, never
/// as the operator's actual paths.
fn must_not_mask() -> Vec<(&'static str, String)> {
    [
        ("N01", "NotificationDigestV2ComponentWrapper.tsx"),
        ("N02", "RevenueCatSubscriptionGatewayAdapter2.ts"),
        ("N03", "apps/mobile/components/DailyBriefingCarouselV3.tsx"),
        ("N04", "supabase/functions/revenuecat-webhook/index.ts"),
        ("N05", "packages/core/src/scheduling/NotificationBatchScheduler2.test.ts"),
        ("N06", "Also in the media section, only the today's playlist should be free."),
        ("N07", "In the profile section is the avatar upload absolutely necessary?"),
        ("N08", "55 edits after the last commit \u{2014} unresolved"),
        ("N09", "17 edits after the last commit \u{2014} unresolved \u{b7} apps/mobile/lib/revenuecat.ts"),
        ("N10", "12 edits \u{b7} DailyBriefingCarouselV3.tsx"),
        ("N11", "chore(mobile): bump build number and regenerate eas.json"),
        ("N12", "feat(v1-launch): grant Pro to all users and fix chart rendering"),
        ("N13", "cd /Users/someone/dev/atlas_playstore && git status --porcelain"),
        ("N14", "pnpm tsx scripts/trigger-daily-briefing.ts 2>&1 | head -40"),
        ("N15", "src/components/NotificationDigest.tsx"),
        ("N16", "https://example.com/docs/getting-started"),
        ("N17", "the build succeeded in 42 seconds"),
        ("N18", "Make the notification digest work"),
        ("N19", "36e7c68565cc2179eb76be85cf160a869e81864ea8e70b13f8761f59a847c895"),
        ("N20", "3c1d9a52-7b4e-4f08-9d26-1e5a8c07b6f3"),
        ("N21", "01KZVXY6PPPJPK837HCKC9MWJ8"),
        ("N22", "01KYNCMXA1A7KB84KH2Z7CKHRP"),
        ("N23", "compiled at chain position 36e7c68565cc2179eb76be85cf160a869e81864ea8e70b13f8761f59a847c895 over 17708 recorded events"),
        ("N24", "the record's last unresolved edit run is in session 6b2e0f14-9a7d-4c35-8e51-d4f3a9c28b70"),
        ("N25", "obl1:361140cc8d7b52c956a79e3fbc6d5de7e8dbc93f53129d2b5476e4c59fcfe7e5"),
        ("N26", "wsid1:abababababababababababababababababababababababababababababababab"),
        ("N27", "spkf_000000000000"),
        ("N28", "cvrd_9f2a7c1b4e60"),
        ("N29", "tlc_aaaaaaaaaaaa"),
        ("N30", "TLPB1 TLCAP_V2 TLSNAP_V1"),
        ("N31", "event_01KYNCMXA1A7KB84KH2Z7CKHRP"),
        ("N32", "693d769287d1a6bc7fa99a2d574d239828ad0aca"),
        ("N33", "693d769 c5acbc1 a8826d8 0a79de0"),
        ("N34", "sha256:36e7c68565cc2179eb76be85cf160a869e81864ea8e70b13f8761f59a847c895"),
        ("N35", "/Users/someone/.claude/projects/-Users-someone/698332f2-0000-4000-8000-4d5df7ba676d.jsonl"),
        ("N36", "-Users-someone-Desktop-Telescope-project--claude-worktrees-competent-johnson-facb61"),
        ("N37", "nomic-embed-text@sha256:0a109f42e59f"),
        ("N38", "/Users/someone/Developer/private-beta/apps/web/src/lib/task-board.ts"),
        ("N39", "apps/web/tests/unit/task-board.test.ts"),
        ("N40", "/Volumes/Drive/Telescope_project/supabase/functions/ask-chart-orchestrator/index.ts"),
        ("N41", "aws ecs describe-task-definition --task-definition staging-web --region ap-south-1"),
        ("N42", "aws ecs register-task-definition --cli-input-json file://task-def.json"),
        ("N43", "feat/task-blocking-board"),
        ("N44", "disk-capacity disk-backed no-risk-score"),
        ("N45", "task-list.md src/tasks/task-list.md"),
        ("N46", "risk-analysis-2026.md"),
        ("N47", "monkeyJump.tsx components/keyJson.ts surveyJson.ts"),
        ("N48", "mcp__ccd_session__mark_chapter"),
        ("N49", "mcp__bf7c680d-5fdc-5ef4-b4a0-abadb619bf0a__send_later"),
        // Case-sensitivity is intent: a case-insensitive ASIA masks every
        // IANA Asia timezone.
        ("N50", "Asia/Kolkata America/New_York"),
        ("N51", "TZ=Asia/Kolkata"),
        ("N52", "ap-south-1 us-east-1 ap-southeast-2"),
        ("N53", "17 edits after the last commit \u{2014} unresolved \u{b7} apps/web/src/lib/task-board.ts, apps/web/tests/unit/task-board.test.ts"),
        ("N54", "chacha20poly1305 = \"0.10.1\" ed25519-dalek = \"2.2.0\" thruline-core serde_json insta ureq"),
        ("N55", "https://doc.rust-lang.org/std/collections/struct.BTreeSet.html"),
        ("N56", "docs/P0-BUILD-BRIEF.md crates/core/src/redact.rs"),
        ("N57", "deepseek-r1:8b qwen2.5:7b-instruct nli-deberta-v3-xsmall"),
        ("N58", "Q29udGludWl0eSBpcyB0aGUgcHJvZHVjdCBoZXJlLg=="),
        ("N59", "grep -c 'sk-veil' sectionsSK/sk2.html"),
        // The 38-token family the first repair over-masked.
        ("N60", "data-sk-seg data-sk-provider data-sk-cheat"),
        ("N61", "data-sk-drop=\"gusto\""),
        ("N64", "deno check supabase/functions/ask-chart-orchestrator/index.ts"),
        ("N65", "REGION_ASIA_PACIFIC regions/ASIA.json"),
        ("N66", "docs/recon/c44-kill9-demo.txt"),
        ("N67", "--- front matter --- and a ----- markdown rule"),
        ("N68", "risk-score task-board disk-usage"),
        ("N69", "https://github.com/rust-lang/rust/issues/12345"),
        ("N70", "SELECT COUNT(*) FROM events WHERE tool_name IN ('Edit','Write','Read')"),
        ("N71d", "--- a/crates/core/src/redact.rs"),
        ("N72d", "+++ b/crates/core/src/redact.rs"),
        ("N73", "| ---- | ---- |"),
        ("N74", "data-testid=\"task-list\""),
        ("N75", "docs/UX-DOCTRINE.md#RA6"),
        ("N78", "SK-2026 sprint SK_TOTAL ASK-42"),
        // The PEM over-masking family, measured: firing on the bare
        // literal `-----BEGIN` masked 70 of 76,203 live opening asks.
        ("N79", "-----BEGIN BID-----\nSUBMITTAL DATA FOR REVIEW"),
        ("N80", "-----BEGIN CERTIFICATE-----\nMIICabcdefghijklmnopqrst"),
        ("N81", "grep -n -- '-----BEGIN' crates/core/src/redact.rs"),
        ("N82", "the file starts with -----BEGIN and then base64"),
        ("N83", "-----BEGIN PUBLIC KEY-----\nMIIBIjANBgkqhkiG9w0BAQEFAAO"),
        // Bare banners: prose about keys, not keys (A209-C6/B8-3).
        ("N84", "-----BEGIN RSA PRIVATE KEY-----"),
        ("N85", "grep -rn -- '-----BEGIN PRIVATE KEY-----' src/"),
        ("N86", "// starts with -----BEGIN OPENSSH PRIVATE KEY----- in PEM form"),
    ]
    .iter()
    .map(|(id, s)| (*id, (*s).to_string()))
    .collect()
}

/// Named expected exceptions: they fail before AND after the A209 repair,
/// they are enumerated in `redact.rs`'s measured-misses section, and curing
/// them needs a stricter body charset that would risk missing a real
/// all-alphabetic key. Asserted to STILL fail, so a future cure surfaces
/// here rather than passing silently.
const EXPECTED_EXCEPTIONS: &[(&str, &str)] = &[
    ("N62", "id=\"sk-fingerprint-text\""),
    ("N63", "sk-veil sk-body sk-market-untouched"),
];

#[test]
fn every_must_mask_case_is_masked_and_named() {
    let mut failures = Vec::new();
    let cases = must_mask();
    for (id, input, expect) in &cases {
        let (out, fired, n) = mask(input);
        let leaked = out.contains("TLCANARY") || out.contains(AWS) || out.contains(ASIA);
        if n == 0 || !fired.contains(*expect) || leaked {
            failures.push(format!(
                "{id}: expected {expect}, got {fired:?} n={n} -> {out}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} MUST-MASK cases failed:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
    assert!(
        cases.len() >= 76,
        "the corpus shrank to {} cases",
        cases.len()
    );
}

#[test]
fn every_must_not_mask_case_survives_untouched() {
    let mut failures = Vec::new();
    let cases = must_not_mask();
    for (id, input) in &cases {
        let (out, fired, n) = mask(input);
        if n != 0 {
            failures.push(format!("{id}: fired {fired:?} -> {out}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} MUST-NOT-MASK cases regressed:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
    assert!(
        cases.len() >= 74,
        "the corpus shrank to {} cases",
        cases.len()
    );
}

#[test]
fn the_named_exceptions_still_fail_and_are_disclosed() {
    let module =
        std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/redact.rs"))
            .expect("read redact.rs");
    for (id, input) in EXPECTED_EXCEPTIONS {
        assert_ne!(
            mask(input).2,
            0,
            "{id} now passes — cure it in the corpus and in the measured-misses \
             section rather than leaving a stale exception"
        );
    }
    // And the module discloses them, per D4.
    assert!(
        module.contains("sk-fingerprint-text") && module.contains("sk-market-untouched"),
        "the standing false positives are not enumerated in measured misses"
    );
}

/* ── the mechanical enumeration (the structural instruction) ────────── */

/// **Behavioural** refusal tests, one per live Gate.
///
/// The previous protection was `window.contains("resembles_known_credential")`
/// over a fixed 1600-character window. An attacker removed the real
/// `|| resembles_known_credential(trimmed)` arm from `safe_model_string`,
/// reinstating an earlier round's defect verbatim, left the identifier in
/// an adjacent COMMENT, and got the whole suite green. A `use` line, a doc
/// comment, a string literal or a neighbouring function satisfied it
/// equally — and it was the sole protection on two of three live Gates,
/// neither of which had any behavioural test at all.
///
/// These call each gate and assert refusal. A comment cannot satisfy them.
#[test]
fn every_live_gate_behaviourally_refuses_bare_credential_prefixes() {
    const BARE: &[&str] = &[
        "sk-",
        "sk-ant-",
        "ghp_",
        "gho_",
        "ghu_",
        "ghs_",
        "ghr_",
        "github_pat_",
        "AKIA",
        "ASIA",
        "AIza",
        "eyJ",
        "xoxb-",
    ];
    for bare in BARE {
        assert!(
            !thruline_core::annotate::safe_language_model_name_for_test(bare),
            "annotate::safe_language_model_name accepted {bare:?}"
        );
        assert!(
            thruline_core::language::ShadowLabel::new(*bare).is_err(),
            "language::validate_shadow_label accepted {bare:?}"
        );
        assert!(
            !thruline_core::mind_grounded::safe_model_string_for_test(bare),
            "mind_grounded::safe_model_string accepted {bare:?}"
        );
    }
    for real in [gh('p'), gh('u'), gh('r'), oai(), ant(), AWS.to_string()] {
        assert!(!thruline_core::annotate::safe_language_model_name_for_test(
            &real
        ));
        assert!(thruline_core::language::ShadowLabel::new(real.clone()).is_err());
        assert!(!thruline_core::mind_grounded::safe_model_string_for_test(
            &real
        ));
    }
    // Positive controls: each gate still ACCEPTS its own valid input, so a
    // gate that refused everything could not satisfy this test either.
    assert!(thruline_core::annotate::safe_language_model_name_for_test(
        "deepseek-r1:8b"
    ));
    assert!(thruline_core::language::ShadowLabel::new("Refactoring the parser").is_ok());
    assert!(thruline_core::mind_grounded::safe_model_string_for_test(
        "the parser was refactored"
    ));
}

/// The workspace root — derived from the manifest dir, not assumed.
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

/// The fewest `redact::mask` call sites the census may find.
///
/// Measured at commit e43d328f: this census walked 16 members there and found
/// 33 sites (`cargo test -p thruline-core --test
/// a209_masking_corpus`, reading the `a209 census:` line it prints). Raise it
/// when a site is added; lowering it is a decision to record, not a fix.
const MIN_MASK_CALL_SITES: usize = 33;

// C38: Decisions-owned additions only. The original shared registry and its
// assertions stay intact. Each reason names the actual output or refusal use;
// none is an exemption from masking or from the source census below.
const DECISIONS_MASK_CALL_SITES: &[(&str, &str, MaskUse, &str)] = &[
    ("attested_command_adapter.rs", "check_forbidden_at", MaskUse::Redactor,
     "Retains masked raw-source paths and command witness text in findings after exact source checks; mask firing never selects a verdict."),
    ("bridge.rs", "review_terminal_act", MaskUse::Gate,
     "Refuses a changed masked review before display; also refuses known credential prefixes in decision words and action metadata."),
    ("c32_span_scope.rs", "resolve_spans_at", MaskUse::Redactor,
     "Retains masked path copies in exclusion receipts and missing-source diagnostics; raw paths and blob pins resolve the scope separately."),
    ("checks_k_assertions.rs", "finding", MaskUse::Redactor,
     "Retains masked accepted path and exact test-name display copies in structural findings; the AST calculation uses the original input."),
    ("checks_k_fixture_lookup.rs", "scan_inner", MaskUse::Redactor,
     "Retains masked source paths in candidate observations after fixture linkage is computed; the masker cannot certify the candidate."),
    ("checks_visual.rs", "report_from_screens", MaskUse::Redactor,
     "Retains masked routes and full diagnostic finding text after typed screen states have been selected; it never compares the mask count."),
    ("ci_adapter.rs", "finding", MaskUse::Redactor,
     "Retains a masked workflow path beside one of two fixed diagnostic sentences; exact YAML observations determine the CI result first."),
    ("conflict.rs", "authority_witness", MaskUse::Redactor,
     "Retains a masked raw-file display path beside the unchanged authority event ID, line and digest; the displayed path creates no authority."),
    ("conflict.rs", "project_conflicts", MaskUse::Redactor,
     "Retains masked conflict reasons and optional paths after authority and predicate comparison; firing does not determine conflict state."),
    ("context.rs", "mask_json", MaskUse::Redactor,
     "Recursively retains masked JSON string values for a local derived context copy; it does not use firing to authorize a context or decision."),
    ("decide_cmd.rs", "list", MaskUse::Redactor,
     "Retains masked decision spans in terminal lines and JSON rows; recorded revision and lifecycle selection do not inspect masking."),
    ("decide_cmd.rs", "recorded_verdict_json", MaskUse::Redactor,
     "Retains a masked checkable-if display sentence when serving a stored verdict; the stored basis and verdict state remain separate."),
    ("decisions.rs", "confirmation_transport", MaskUse::Redactor,
     "Retains a masked native-availability refusal reason in the Decisions HTTP projection; operating-system guards determine availability."),
    ("decisions.rs", "start_native_confirmation", MaskUse::Redactor,
     "Retains masked owned-job and derived-copy errors before exposing them over Decisions HTTP; the masker never performs a person act."),
    ("general_supersession.rs", "review_details", MaskUse::Gate,
     "Refuses masked predecessor or successor words before exact revision review and also refuses known credential prefixes; no act is created."),
    ("person_threads.rs", "open_for_project", MaskUse::Redactor,
     "Retains a masked source-verified human prompt in an Open thread projection; coverage and completed-edit checks do not inspect firing."),
    ("review_params.rs", "safe_for_terminal", MaskUse::Gate,
     "Refuses masked or known-prefix parameter documents; the exact public Rust-name exception still requires its source and unchanged raw name."),
    ("revival.rs", "project_revivals", MaskUse::Redactor,
     "Retains masked finding paths and terms in an unverifiable revival row; source-verified edit/version evidence determines revival state."),
    ("revival.rs", "witness", MaskUse::Redactor,
     "Retains a masked raw-file display path while preserving event ID, raw line and digest as separate source-witness fields."),
    ("revision_proposal.rs", "append", MaskUse::Redactor,
     "Normalizes the already source-verified native message to its stored redacted form for exact span comparison; raw bytes and chain independently qualify the source and no person authority is granted."),
    ("rule_files.rs", "decode_note", MaskUse::Gate,
     "Refuses masked or known-prefix stored note spans, source paths and reasons before returning an exact-source proposal note."),
    ("rule_files.rs", "ingest", MaskUse::Gate,
     "Refuses masked or known-prefix rule paths and source prose before proposal writes; unrelated valid pinned rules still import."),
    ("rule_files.rs", "note", MaskUse::Redactor,
     "Retains a masked importer diagnostic reason in a proposal-only report; refusal and source binding have already been determined."),
    ("text_exclusions.rs", "path", MaskUse::Redactor,
     "Retains masked path and pattern display copies in not-looked-at receipts; the receipt is not used to grant acceptance authority."),
    ("text_exclusions.rs", "test_spans", MaskUse::Redactor,
     "Retains a masked accepted-scope path in AST exclusion receipts; the original path, exact test identity and byte bounds bind the parse."),
    ("text_exclusions_js.rs", "spans", MaskUse::Redactor,
     "Retains a masked source path in an exact JavaScript test-scope receipt after AST identity and byte-bound calculations."),
];

#[test]
fn additional_decisions_mask_sites_have_owned_scopes_and_guarded_gates() {
    let identities: BTreeSet<_> = DECISIONS_MASK_CALL_SITES
        .iter()
        .map(|(file, name, _, _)| (*file, *name))
        .collect();
    assert_eq!(
        identities.len(),
        26,
        "every C38 Decisions addition has one classification"
    );
    assert_eq!(
        DECISIONS_MASK_CALL_SITES
            .iter()
            .filter(|(_, _, use_, _)| *use_ == MaskUse::Gate)
            .count(),
        5
    );
    assert_eq!(
        DECISIONS_MASK_CALL_SITES
            .iter()
            .filter(|(_, _, use_, _)| *use_ == MaskUse::Redactor)
            .count(),
        21
    );
    for (file, name, use_, reason) in DECISIONS_MASK_CALL_SITES {
        assert!(
            reason.len() > 50,
            "{file}::{name} needs a source-specific reason"
        );
        let relative = match *file {
            "decide_cmd.rs" => "crates/cli/src/decide_cmd.rs".into(),
            "decisions.rs" => "crates/cli/src/uiserve/decisions.rs".into(),
            _ => format!("crates/core/src/decisions/{file}"),
        };
        let source = std::fs::read_to_string(workspace_root().join(&relative))
            .expect("Decisions-owned source");
        let body = function_code(&source, name).expect("complete classified function body");
        assert!(
            body.contains("Masked::mask"),
            "{relative}::{name} must remain a real masker caller"
        );
        if *use_ == MaskUse::Gate {
            assert!(
                body.contains("resembles_known_credential"),
                "{relative}::{name} is a gate and must also refuse known credential prefixes"
            );
        }
    }
}

// C44 §5 authorizes only this append-only registry identity update. The raw
// source census is never normalized: a mask returning to the legacy wrapper
// remains a new, unclassified source site and fails exact equality.
const C44_STORAGE_MASK_IDENTITY: (&str, &str, &str, &str) = (
    "storage.rs",
    "append_revision",
    "append_revision_guarded",
    "The existing span and actor masks moved to the guarded SQL append so exact parent comparison and persistence share a transaction.",
);

fn c44_documented_mask_identities(
    mut documented: BTreeSet<(String, String)>,
) -> BTreeSet<(String, String)> {
    let (file, old, new, reason) = C44_STORAGE_MASK_IDENTITY;
    assert_eq!(
        (file, old, new),
        ("storage.rs", "append_revision", "append_revision_guarded")
    );
    assert!(reason.len() > 50);
    for name in [old, new] {
        let rows: Vec<_> = MASK_CALL_SITES
            .iter()
            .filter(|(f, n, _)| *f == file && *n == name)
            .collect();
        assert_eq!(
            rows.len(),
            1,
            "each migration identity must occur exactly once"
        );
        assert_eq!(rows[0].2, MaskUse::Redactor);
    }
    let source =
        std::fs::read_to_string(workspace_root().join("crates/core/src/decisions/storage.rs"))
            .expect("exact Decisions storage source");
    let flat_body = |name| {
        function_code(&source, name)
            .expect("complete storage function")
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
    };
    let old_body = flat_body(old);
    assert!(!old_body.contains("Masked::"));
    assert!(old_body.ends_with("{append_revision_guarded(ledger,input,None)}"));
    let parent_body = flat_body("append_revision_if_parent");
    assert!(!parent_body.contains("Masked::"));
    assert!(parent_body.ends_with("{append_revision_guarded(ledger,input,Some(parent))}"));
    let new_body = flat_body(new);
    assert_eq!(new_body.matches("Masked::mask(").count(), 2);
    assert_eq!(new_body.matches("Masked::mask(&input.span)").count(), 1);
    assert_eq!(
        new_body
            .matches("Masked::mask(&input.author_actor)")
            .count(),
        1
    );
    assert!(new_body.contains("tx.execute("));
    assert!(new_body.contains("tx.commit()?"));
    let before = documented.clone();
    assert!(documented.remove(&(file.into(), old.into())));
    assert!(documented.contains(&(file.into(), new.into())));
    assert_eq!(documented.len() + 1, before.len());
    assert_eq!(
        before.difference(&documented).cloned().collect::<Vec<_>>(),
        vec![(file.into(), old.into())],
    );
    documented
}

#[test]
fn c44_storage_identity_does_not_hide_a_mask_returning_to_the_old_wrapper() {
    let documented = MASK_CALL_SITES
        .iter()
        .map(|(f, n, _)| ((*f).to_string(), (*n).to_string()))
        .chain(
            DECISIONS_MASK_CALL_SITES
                .iter()
                .map(|(f, n, _, _)| ((*f).to_string(), (*n).to_string())),
        )
        .collect();
    let documented = c44_documented_mask_identities(documented);
    let found = derive_call_sites(
        &member_sources(),
        Some((
            "storage.rs",
            "\nfn append_revision(v: &str) { let _ = crate::redact::Masked::mask(v); }\n",
        )),
    );
    assert!(found.contains(&("storage.rs".into(), "append_revision".into())));
    assert!(!documented.contains(&("storage.rs".into(), "append_revision".into())));
    assert_ne!(
        found, documented,
        "the raw source site must not inherit its moved identity"
    );
}

/// Every member crate's `src`, read from the workspace manifest.
///
/// The parse splits ONE line on `[` and `]`, which is why the root
/// `Cargo.toml` says `members` must stay on one line. A multi-line array
/// would shrink the walk without a word, so the shape is asserted here and
/// the parsed list is checked against an independent derivation — every
/// `crates/*/Cargo.toml` that declares a `[package]` — rather than trusted.
fn member_sources() -> Vec<PathBuf> {
    let root = workspace_root();
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).expect("workspace manifest");
    let member_lines: Vec<&str> = manifest
        .lines()
        .filter(|l| {
            l.trim_start()
                .strip_prefix("members")
                .is_some_and(|rest| rest.trim_start().starts_with('='))
        })
        .collect();
    assert_eq!(
        member_lines.len(),
        1,
        "the workspace manifest must have exactly one `members = [...]` line, \
         found {member_lines:?}"
    );
    let line = member_lines[0];
    let after_eq = line.split_once('=').map(|(_, r)| r.trim()).unwrap_or("");
    assert!(
        after_eq.starts_with('[') && after_eq.trim_end().ends_with(']'),
        "`members` must open AND close its array on ONE line — this census \
         splits that line on `[` and `]`, and a multi-line array would shrink \
         the walk to nothing. Found: {line:?}"
    );
    let inner = line
        .split_once('[')
        .and_then(|(_, r)| r.split_once(']'))
        .map(|(m, _)| m)
        .expect("members list");
    // Manifest order is kept for the walk; the set is for the comparison.
    let listed: Vec<String> = inner
        .split(',')
        .map(|m| m.trim().trim_matches('"').trim())
        .filter(|m| !m.is_empty())
        .map(str::to_string)
        .collect();
    assert!(
        !listed.is_empty(),
        "the parsed members list is empty: {line:?}"
    );
    let names: BTreeSet<String> = listed.iter().cloned().collect();
    assert_eq!(
        names.len(),
        listed.len(),
        "the members line names a crate twice: {listed:?}"
    );

    // The independent derivation: the crates the workspace actually has,
    // read from the directory tree and not from the line above.
    let on_disk: BTreeSet<String> = std::fs::read_dir(root.join("crates"))
        .expect("crates/")
        .flatten()
        .filter(|e| {
            std::fs::read_to_string(e.path().join("Cargo.toml"))
                .is_ok_and(|t| t.lines().any(|l| l.trim() == "[package]"))
        })
        .map(|e| format!("crates/{}", e.file_name().to_string_lossy()))
        .collect();
    assert!(
        !on_disk.is_empty(),
        "no crates/*/Cargo.toml declares a [package]"
    );
    assert_eq!(
        names.len(),
        on_disk.len(),
        "the members line names {} crates but crates/ holds {} packages.\n\
         in members only: {:?}\n on disk only: {:?}",
        names.len(),
        on_disk.len(),
        names.difference(&on_disk).collect::<Vec<_>>(),
        on_disk.difference(&names).collect::<Vec<_>>()
    );
    assert_eq!(
        names, on_disk,
        "the members line and crates/ name different crates"
    );

    let members: Vec<PathBuf> = listed.iter().map(|m| root.join(m).join("src")).collect();
    // `walk` returns quietly on an unreadable directory, so a member whose
    // `src` is missing would be skipped rather than reported.
    for src in &members {
        assert!(src.is_dir(), "member source dir missing: {}", src.display());
    }
    members
}

/// Strip `#[cfg(test)]` items by brace depth rather than truncating the
/// file at the first marker.
///
/// The previous version `break`-ed at the first column-0 `#[cfg(test)]`.
/// In `derive.rs` that marker is a `#[cfg(test)] const` at line 1553 of
/// 5746, so **4,193 lines went unscanned** — the walk offered as the cure
/// for false enumeration was itself under-counting.
fn strip_cfg_test(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        if !line.trim_start().starts_with("#[cfg(test)]") {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        // Skip the attribute and the item it guards, tracking braces.
        let mut depth = 0i32;
        let mut started = false;
        for item in lines.by_ref() {
            depth += item.matches('{').count() as i32;
            depth -= item.matches('}').count() as i32;
            if item.contains('{') {
                started = true;
            }
            if started && depth <= 0 {
                break;
            }
            if !started && item.trim_end().ends_with(';') {
                break; // a `#[cfg(test)] const X = ...;` item
            }
        }
    }
    out
}

/// Re-derive **every** non-test call site of `redact::mask` / `mask_all`
/// across every workspace member and require it to equal
/// [`MASK_CALL_SITES`].
///
/// Three rounds of this stage failed on false enumerations. The first
/// replacement for the hand list keyed on the `.2` spelling and
/// under-counted; the second walked only `crates/core/src`, truncated each
/// file at the first `#[cfg(test)]`, matched one literal spelling, and
/// still described itself in four documents as re-deriving "the
/// workspace". This one walks the manifest's members, strips cfg(test) by
/// brace depth, matches both entry points, and is checked by a negative
/// control below.
#[test]
fn every_mask_call_site_is_classified() {
    let sources = member_sources();
    let found = derive_call_sites(&sources, None);
    // The census size, printed so the gate log carries it. Written straight
    // to the stderr handle rather than through `eprintln!`: libtest captures
    // the print macros of a passing test, and the gate runs without
    // `--nocapture`, so an `eprintln!` here would only ever appear on failure.
    {
        use std::io::Write as _;
        let _ = writeln!(
            std::io::stderr().lock(),
            "a209 census: {} workspace members walked, {} redact::mask call sites found \
             (floor {MIN_MASK_CALL_SITES})",
            sources.len(),
            found.len()
        );
    }
    let documented: BTreeSet<(String, String)> = MASK_CALL_SITES
        .iter()
        .map(|(f, n, _)| ((*f).to_string(), (*n).to_string()))
        .collect();
    let documented: BTreeSet<_> = documented
        .into_iter()
        .chain(
            DECISIONS_MASK_CALL_SITES
                .iter()
                .map(|(f, n, _, _)| ((*f).to_string(), (*n).to_string())),
        )
        .collect();
    let documented = c44_documented_mask_identities(documented);
    assert_eq!(
        found,
        documented,
        "the redact::mask call sites drifted from MASK_CALL_SITES.\n\
         found but unclassified: {:?}\n classified but not found: {:?}\n\
         Classify the new site (Gate / Redactor / Inert); a Gate must also \
         consult resembles_known_credential.",
        found.difference(&documented).collect::<Vec<_>>(),
        documented.difference(&found).collect::<Vec<_>>()
    );
    // The walk must actually be reading a real corpus, not an empty one.
    // The floor is the count measured, not a guess: a shrinking walk that
    // also lost its MASK_CALL_SITES rows would otherwise pass the equality.
    assert!(
        found.len() >= MIN_MASK_CALL_SITES,
        "the walk found only {} sites, below the measured floor of \
         {MIN_MASK_CALL_SITES} — it is not reaching the sources",
        found.len()
    );
}

/// The negative control. Without it the walk is a heuristic asserting its
/// own completeness — the failure being repaired.
#[test]
fn the_enumeration_fails_when_an_unclassified_site_is_injected() {
    let sources = member_sources();
    let documented: BTreeSet<(String, String)> = MASK_CALL_SITES
        .iter()
        .map(|(f, n, _)| ((*f).to_string(), (*n).to_string()))
        .collect();
    let documented: BTreeSet<_> = documented
        .into_iter()
        .chain(
            DECISIONS_MASK_CALL_SITES
                .iter()
                .map(|(f, n, _, _)| ((*f).to_string(), (*n).to_string())),
        )
        .collect();
    let documented = c44_documented_mask_identities(documented);
    // Each injection is a call site the walk must SEE. They cover the
    // spellings that previously slipped past: a site far past a
    // `#[cfg(test)]` marker, a crates/cli site, `mask_all`, and the
    // `pub(crate) fn` / `async fn` name forms.
    let injections = [
        (
            "derive.rs",
            "past_cfg_test_marker",
            "pub(crate) fn past_cfg_test_marker(v: &str) -> bool {\n    crate::redact::mask(v).2 > 0\n}\n",
        ),
        (
            "cli_main.rs",
            "cli_side_gate",
            "async fn cli_side_gate(v: &str) -> bool {\n    thruline_core::redact::mask(v).2 == 0\n}\n",
        ),
        (
            "language.rs",
            "bulk_redactor",
            "const fn bulk_redactor() {}\nfn bulk_redactor_real(t: &mut [&mut String]) {\n    let _ = crate::redact::mask_all(t);\n}\n",
        ),
    ];
    for (file, func, snippet) in injections {
        let found = derive_call_sites(&sources, Some((file, snippet)));
        assert!(
            found.iter().any(|(_, n)| n == func || n.contains(func)),
            "the walk did not see an injected call site in {file} ({func}) — \
             it cannot claim completeness"
        );
        assert_ne!(
            found, documented,
            "an unclassified site injected into {file} left the derivation \
             unchanged — the enumeration is not actually checking anything"
        );
    }
}

/// Walk the given sources, optionally appending `injection` to one file's
/// text in memory (nothing is written to disk).
fn derive_call_sites(
    sources: &[PathBuf],
    injection: Option<(&str, &str)>,
) -> BTreeSet<(String, String)> {
    let mut found: BTreeSet<(String, String)> = BTreeSet::new();
    let mut injected_here = injection.is_none();
    for src in sources {
        walk(src, &mut |path, raw| {
            let file = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if file == "redact.rs" {
                return; // the module itself is not a caller
            }
            let mut raw_with_injection = raw.to_string();
            if let Some((target, snippet)) = injection {
                // `cli_main.rs` stands for "any file in the cli crate".
                let matches_target = file == target
                    || (target == "cli_main.rs"
                        && path.components().any(|c| c.as_os_str() == "cli")
                        && file == "main.rs");
                if matches_target && !injected_here {
                    // Injected into the RAW text so it traverses
                    // strip_cfg_test. The previous version appended AFTER
                    // the strip, so the case named `past_cfg_test_marker`
                    // never crossed a marker and the control could not
                    // detect a regression of the defect it exists for
                    // (A209-C4/B6).
                    raw_with_injection.push_str(snippet);
                    injected_here = true;
                }
            }
            let text = strip_cfg_test(&raw_with_injection);
            let mut current_fn = String::new();
            for line in text.lines() {
                let t = line.trim_start();
                for form in [
                    "pub(crate) fn ",
                    "pub(super) fn ",
                    "pub async fn ",
                    "pub const fn ",
                    "pub fn ",
                    "async fn ",
                    "const fn ",
                    "fn ",
                ] {
                    if let Some(rest) = t.strip_prefix(form) {
                        current_fn = rest
                            .split(['(', '<'])
                            .next()
                            .unwrap_or("")
                            .trim()
                            .to_string();
                        break;
                    }
                }
                if t.starts_with("//") {
                    continue;
                }
                // Both entry points, and without requiring the paren so a
                // re-exported or aliased call is still seen.
                // `Masked::mask` is the newtype constructor — the same
                // masker, a different spelling. A209-C7 converted two
                // derive.rs sites to it, and an enumeration keyed on one
                // spelling would have quietly lost them.
                if line.contains("redact::mask")
                    || line.contains("mask_all(")
                    || line.contains("Masked::mask(")
                    || line.contains("Masked::unknown(")
                {
                    found.insert((file.clone(), current_fn.clone()));
                }
            }
        });
    }
    if let Some((target, _)) = injection {
        assert!(
            injected_here,
            "the negative control never found {target} to inject into"
        );
    }
    found
}

/// A real sweep for truncation sites, whitespace-normalized so rustfmt's
/// `.chars()\n.take(` split cannot hide one.
fn derive_truncation_sites(injection: Option<(&str, &str)>) -> BTreeSet<(String, String)> {
    let mut found: BTreeSet<(String, String)> = BTreeSet::new();
    let mut injected_here = injection.is_none();
    for src in member_sources() {
        walk(&src, &mut |path, raw| {
            let file = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if file == "redact.rs" {
                return; // the masker owns the marker-safe cut itself
            }
            let mut raw_with_injection = raw.to_string();
            if let Some((target, snippet)) = injection {
                if file == target && !injected_here {
                    raw_with_injection.push_str(snippet);
                    injected_here = true;
                }
            }
            let text = strip_cfg_test(&raw_with_injection);
            let mut current_fn = String::new();
            let mut pending = String::new();
            for line in text.lines() {
                let t = line.trim_start();
                for form in [
                    "pub(crate) fn ",
                    "pub(super) fn ",
                    "pub async fn ",
                    "pub const fn ",
                    "pub fn ",
                    "async fn ",
                    "const fn ",
                    "fn ",
                ] {
                    if let Some(rest) = t.strip_prefix(form) {
                        current_fn = rest
                            .split(['(', '<'])
                            .next()
                            .unwrap_or("")
                            .trim()
                            .to_string();
                        break;
                    }
                }
                if t.starts_with("//") {
                    continue;
                }
                pending.push_str(line);
                let flat: String = pending.chars().filter(|c| !c.is_whitespace()).collect();
                // A209-C5/B7-9: matching PER LINE could not see
                // `.chars()` on one line followed by `.take(` on the next
                // — the form `candidates.rs` was already written in, so
                // the sweep was blind to one of the four sites
                // TRUNCATION_SITES itself enumerates. A rolling window
                // across the accumulating function body sees both forms.
                // `.chars()` followed ANYWHERE IN THE SAME CHAIN by `.take(`,
                // not the adjacent literal. Track G's `head_token` flattens to
                // `.chars().take_while(…).take(48)`, and the literal
                // `.chars().take(` is not in it — so the census walked past a
                // rendering truncation while finding its neighbour. Third
                // instance today of the same spelling gap (D30): a guard whose
                // search space is a spelling rather than a shape.
                //
                // "Same chain" is "before the next `;`", which is what stops a
                // `.chars()` in one statement pairing with a `.take(` in an
                // unrelated one. Over-matching would only add sites to a list
                // somebody must then justify, so the bound errs toward more
                // enumeration, not less.
                let chained_take = flat.find(".chars()").is_some_and(|c| {
                    let rest = &flat[c..];
                    let stmt = rest.split(';').next().unwrap_or(rest);
                    stmt.contains(".take(")
                });
                // Decisions' one-line renderers keep the Chars iterator in a
                // local, then call `chars.by_ref().take(n)` in the next
                // statement. The same-chain search above stops at `;` and
                // therefore missed both bounded_span and bounded. This is
                // the observed split-iterator shape, kept narrow so a parser
                // consuming four hex digits through `it.by_ref().take(4)`
                // does not become a rendered-text truncation by spelling.
                let split_chars_take = flat.contains("letmutchars=")
                    && flat.contains(".chars();")
                    && flat.contains("chars.by_ref().take(");
                if chained_take
                    || split_chars_take
                    || flat.contains("truncate_respecting_markers(")
                    || flat.contains(".truncate(")
                {
                    found.insert((file.clone(), current_fn.clone()));
                    pending.clear();
                }
                if pending.len() > 4000 {
                    pending.clear();
                }
            }
        });
    }
    if let Some((target, _)) = injection {
        assert!(injected_here, "the control never found {target}");
    }
    found
}

/// The truncation census, for real.
///
/// The previous version iterated only the entries already in the const and
/// closed with `assert_eq!(documented.len(), 4)` — a tautology that passed
/// while an injected `chars().take(120)` sat in `operations.rs`
/// (A209-C4/B5), while `redact.rs` told readers this test swept the
/// workspace. It does now, and it has a negative control of its own.
fn function_code(text: &str, name: &str) -> Option<String> {
    // Keep offsets, but remove comments and string/character literals so a
    // brace or a claimed mask inside prose cannot terminate or satisfy the
    // function check. Unlike a fixed character window, this reaches the
    // actual closing brace and cannot borrow a mask from the next function.
    let bytes = text.as_bytes();
    let mut code = bytes.to_vec();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        if bytes[i..].starts_with(b"//") {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        } else if bytes[i..].starts_with(b"/*") {
            i += 2;
            let mut depth = 1;
            while i < bytes.len() && depth > 0 {
                if bytes[i..].starts_with(b"/*") {
                    depth += 1;
                    i += 2;
                } else if bytes[i..].starts_with(b"*/") {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
        } else {
            let mut quote = i;
            if bytes[i] == b'r' {
                quote += 1;
                while quote < bytes.len() && bytes[quote] == b'#' {
                    quote += 1;
                }
            }
            if bytes[i] == b'r' && quote < bytes.len() && bytes[quote] == b'"' {
                let hashes = quote - i - 1;
                i = quote + 1;
                while i < bytes.len() {
                    if bytes[i] == b'"'
                        && bytes
                            .get(i + 1..i + 1 + hashes)
                            .is_some_and(|s| s.iter().all(|b| *b == b'#'))
                    {
                        i += 1 + hashes;
                        break;
                    }
                    i += 1;
                }
            } else if bytes[i] == b'"' {
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i = (i + 2).min(bytes.len());
                    } else if bytes[i] == b'"' {
                        i += 1;
                        break;
                    } else {
                        i += 1;
                    }
                }
            } else if bytes[i] == b'\'' {
                // A lifetime is not a character literal. Only skip a quoted
                // single Unicode scalar or an escaped character.
                let end = bytes[i + 1..]
                    .iter()
                    .position(|b| *b == b'\'' || *b == b'\n')
                    .map(|n| i + 1 + n);
                if let Some(end) = end.filter(|end| bytes[*end] == b'\'') {
                    let inside = &text[i + 1..end];
                    if inside.chars().count() == 1 || inside.starts_with('\\') {
                        i = end + 1;
                    }
                }
                if i == start {
                    i += 1;
                    continue;
                }
            } else {
                i += 1;
                continue;
            }
        }
        code[start..i].fill(b' ');
    }
    let code = String::from_utf8(code).ok()?;
    let at = code.find(&format!("fn {name}("))?;
    let begin = at + code[at..].find('{')?;
    let mut depth = 0;
    for (offset, byte) in code.as_bytes()[begin..].iter().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(
                        code[at..=begin + offset]
                            .chars()
                            .filter(|c| !c.is_whitespace())
                            .collect(),
                    );
                }
            }
            _ => {}
        }
    }
    None
}

fn masks_before_cut(window: &str) -> bool {
    let mask_at = window
        .find("redact::mask")
        .or_else(|| window.find("mask_recorded"))
        .or_else(|| window.find("Masked::mask"));
    let take_at = window
        .find(".chars().take(")
        .or_else(|| window.find("chars.by_ref().take("))
        .or_else(|| window.find("truncate_respecting_markers("));
    match (mask_at, take_at) {
        (Some(m), Some(t)) => m < t,
        (Some(_), None) => true,
        _ => false,
    }
}

#[test]
fn mask_order_guard_covers_the_whole_function_without_borrowing_prose_or_neighbors() {
    let padding = "let untouched = 0;\n".repeat(500);
    let good = format!("fn guarded(text: &str) {{ {padding} let masked = redact::mask(text); truncate_respecting_markers(&masked, 4); }}");
    assert!(masks_before_cut(
        &function_code(&good, "guarded").expect("whole function")
    ));
    for bad in [
        "fn guarded(text: &str) { text.chars().take(4); } fn later() { redact::mask(text); }",
        "fn guarded(text: &str) { text.chars().take(4); redact::mask(text); }",
        "fn guarded(text: &str) { let prose = r###\"} redact::mask(text) {\"###; /* redact::mask(text) */ text.chars().take(4); }",
    ] {
        assert!(!masks_before_cut(&function_code(bad, "guarded").expect("function")), "{bad}");
    }
    assert!(function_code("fn guarded() {", "guarded").is_none());
}

#[test]
fn every_truncation_site_that_feeds_rendered_text_is_enumerated() {
    let found = derive_truncation_sites(None);
    let documented: BTreeSet<(String, String)> = TRUNCATION_SITES
        .iter()
        .map(|(f, n, _)| ((*f).to_string(), (*n).to_string()))
        .collect();
    let out_of_band: BTreeSet<(String, String)> = NON_RENDERING_TRUNCATIONS
        .iter()
        .map(|(f, n, _)| ((*f).to_string(), (*n).to_string()))
        .collect();
    // The third list (DECISIONS #114): rendering, unmasked, safe by INPUT.
    let conditional: BTreeSet<(String, String)> = CONDITIONAL_TRUNCATIONS
        .iter()
        .map(|c| (c.file.to_string(), c.func.to_string()))
        .collect();
    let unaccounted: Vec<_> = found
        .difference(&documented)
        .filter(|k| !out_of_band.contains(*k))
        .filter(|k| !conditional.contains(*k))
        .collect();
    assert!(
        unaccounted.is_empty(),
        "truncation sites in none of TRUNCATION_SITES, NON_RENDERING_TRUNCATIONS or \
         CONDITIONAL_TRUNCATIONS: {unaccounted:?}"
    );

    // Constraint 1: an entry names a CONDITION, not a site. An empty condition
    // is an entry that says nothing, which is the shape a list rots into.
    for c in CONDITIONAL_TRUNCATIONS {
        assert!(
            !c.condition.trim().is_empty() && !c.cuts.trim().is_empty(),
            "{}::{} is in CONDITIONAL_TRUNCATIONS with no stated condition",
            c.file,
            c.func
        );
    }

    // Constraint 2: where a mechanical guard exists it RUNS HERE, so the entry
    // fails the day its premise stops being true.
    for c in CONDITIONAL_TRUNCATIONS {
        // The linkage check runs on its own axis, whatever the guard says.
        if let Some(manifest) = c.linkage {
            let path = workspace_root().join(manifest);
            let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
                panic!("{}::{}'s guard cannot read {manifest}: {e}", c.file, c.func)
            });
            let deps = text
                .split("[dependencies]")
                .nth(1)
                .unwrap_or("")
                .split("\n[")
                .next()
                .unwrap_or("");
            for captured in ["thruline-ledger", "thruline-capture", "thruline-core"] {
                assert!(
                    !deps.contains(captured),
                    "{}::{} is declared safe because its input cannot carry a credential, \
                     and {manifest} now depends on {captured}. The condition has stopped \
                     being true; re-classify the site rather than widening this guard.",
                    c.file,
                    c.func
                );
            }
        }
    }

    // A `TestExists` guard is satisfied when the named test is in this tree.
    // It is not re-run here — the workspace gate runs it — and an entry whose
    // test has not arrived reports UNGUARDED with that reason rather than
    // failing, so a guard living on another track's branch flips to guarded the
    // moment it lands, with no hand edit.
    let test_present = |file: &str, func: &str| -> bool {
        std::fs::read_to_string(workspace_root().join(file))
            .map(|t| t.contains(&format!("fn {func}(")))
            .unwrap_or(false)
    };

    // Constraint 3: the unguarded entries are PRINTED every run, by name.
    // An unguarded entry is a promise somebody made, and a promise nobody reads
    // is how a list rots. Rule 12, applied to the list itself.
    let unguarded: Vec<String> = CONDITIONAL_TRUNCATIONS
        .iter()
        .filter_map(|c| match c.guard {
            TruncationGuard::Unguarded => {
                Some(format!("{}::{} (no mechanical guard)", c.file, c.func))
            }
            TruncationGuard::TestExists { file, func } if !test_present(file, func) => {
                Some(format!(
                    "{}::{} (guard {file}::{func} is not in this tree yet)",
                    c.file, c.func
                ))
            }
            _ => None,
        })
        .collect();
    println!(
        "CONDITIONAL_TRUNCATIONS: {} entr(ies), {} UNGUARDED{}{}",
        CONDITIONAL_TRUNCATIONS.len(),
        unguarded.len(),
        if unguarded.is_empty() { "" } else { ": " },
        unguarded.join(", ")
    );
    for (file, func, order) in TRUNCATION_SITES {
        assert!(order.contains("masks"), "{file}::{func} states no order");
        // Resolve across EVERY workspace member, not just `crates/core/src`.
        // The walk that finds these sites already covers all members, but this
        // verification assumed core — so a truncation site in any other crate
        // could be listed and then never checked, or (as happened with
        // `capture/src/redaction.rs`) could not be listed at all. The census
        // and its verification now cover the same ground.
        let text = member_sources()
            .iter()
            .flat_map(|src| {
                let mut hits: Vec<String> = Vec::new();
                walk(src, &mut |path, raw| {
                    if path.file_name().is_some_and(|f| f == *file) {
                        hits.push(raw.to_string());
                    }
                });
                hits
            })
            .find(|raw| raw.contains(&format!("fn {func}(")))
            .unwrap_or_else(|| panic!("{file}::{func} not found in any workspace member"));
        let window = function_code(&text, func)
            .unwrap_or_else(|| panic!("complete body of {file}::{func} not found"));
        assert!(
            masks_before_cut(&window),
            "{file}::{func} truncates without masking first"
        );
    }
}

/// The census's own negative control — the shape `MASK_CALL_SITES` uses,
/// which works, copied because the census had none and was a tautology.
#[test]
fn the_truncation_census_fails_when_an_unenumerated_cut_is_injected() {
    let baseline = derive_truncation_sites(None);
    let injected = derive_truncation_sites(Some((
        "operations.rs",
        "pub fn injected_preview2(s: &str) -> String { s.chars().take(120).collect() }\n",
    )));
    assert!(
        injected.iter().any(|(_, n)| n == "injected_preview2"),
        "the census did not see an injected truncation site"
    );
    assert_ne!(
        baseline, injected,
        "an injected cut left the census unchanged — it is not checking anything"
    );
}

#[test]
fn split_char_iterator_cut_is_enumerated_and_requires_mask_first() {
    let baseline = derive_truncation_sites(None);
    assert!(
        !baseline.contains(&("premise.rs".into(), "normalise_tail".into())),
        "consuming four hex digits while parsing is not a rendered truncation"
    );
    let injected = derive_truncation_sites(Some((
        "decide_cmd.rs",
        "fn injected_split_iterator(text: &str) -> String {\n    let mut chars = text.chars();\n    chars.by_ref().take(12).collect()\n}\n",
    )));
    assert!(
        injected.iter().any(
            |(file, function)| file == "decide_cmd.rs" && function == "injected_split_iterator"
        ),
        "the census missed a split Chars iterator cut"
    );
    assert_ne!(
        baseline, injected,
        "the split iterator injection changed no census result"
    );

    let good = "fn split(text: &str) -> String { let masked = Masked::mask(text).into_text(); let mut chars = masked.chars(); chars.by_ref().take(12).collect() }";
    let bad = "fn split(text: &str) -> String { let mut chars = text.chars(); let cut: String = chars.by_ref().take(12).collect(); Masked::mask(&cut).into_text() }";
    assert!(masks_before_cut(
        &function_code(good, "split").expect("function")
    ));
    assert!(!masks_before_cut(
        &function_code(bad, "split").expect("function")
    ));
}

fn walk(dir: &Path, f: &mut impl FnMut(&Path, &str)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, f);
        } else if path.extension().is_some_and(|e| e == "rs") {
            if let Ok(text) = std::fs::read_to_string(&path) {
                f(&path, &text);
            }
        }
    }
}

/* ── the strict predicate, directly ─────────────────────────────────── */

#[test]
fn resembles_known_credential_is_strict_where_mask_is_not() {
    // Bare prefixes: not a mask-worthy shape, but disqualifying at a gate.
    for bare in [
        "sk-", "sk-ant-", "ghp_", "ghu_", "ghr_", "AKIA", "ASIA", "eyJ", "AIza", "xoxb-",
    ] {
        assert_eq!(mask(bare).2, 0, "{bare} should not be MASKED");
        assert!(
            resembles_known_credential(bare),
            "{bare} should be REFUSED by a gate"
        );
    }
    // Real credentials: both agree.
    for real in [gh('p'), gh('u'), gh('r'), oai(), ant(), AWS.to_string()] {
        assert!(mask(&real).2 > 0 && resembles_known_credential(&real));
    }
    // Ordinary values a gate must accept — including the model tags and
    // shadow labels the two live gates actually see.
    for benign in [
        "deepseek-r1:8b",
        "qwen2.5:7b-instruct",
        "nli-deberta-v3-xsmall",
        "nomic-embed-text@sha256:0a109f42e59f",
        "risk-score",
        "task-board",
        "disk-usage",
        "Build finished",
        "Asia/Kolkata",
    ] {
        assert_eq!(mask(benign).2, 0, "{benign} masked");
        assert!(
            !resembles_known_credential(benign),
            "{benign} refused by the strict predicate — a valid value is rejected"
        );
    }
}

/// The `annotate` gate had no test at all before A209-C.
#[test]
fn the_language_model_name_gate_refuses_credential_shaped_tags() {
    use thruline_core::annotate::safe_language_model_name_for_test as safe;
    for good in [
        "deepseek-r1:8b",
        "qwen2.5:7b-instruct",
        "llama3.1:8b",
        "nomic-embed-text",
    ] {
        assert!(safe(good), "a valid model tag was refused: {good}");
    }
    for bad in [
        "sk-",
        "ghp_",
        "ghu_",
        "sk-ant-",
        "AKIA",
        &gh('p'),
        &oai(),
        "",
        "model with spaces",
    ] {
        assert!(
            !safe(bad),
            "a credential-shaped or malformed tag passed: {bad}"
        );
    }
}

/// The truncation miss, pinned (A209-C round 3, measured on a live run):
/// a renderer that truncates BEFORE masking can cut a credential below its
/// body threshold. A Google key straddling the 280-character opening-ask
/// boundary shipped in a sealed capsule while six other credentials on the
/// same line masked correctly.
#[test]
fn a_credential_truncated_by_an_upstream_renderer_is_still_masked() {
    // Exactly the live shape: `AIza` + 27 body characters + the ellipsis
    // `candidates` appends at its 280-character cut.
    let truncated = "AIzaTLCANARYaaZZ11bbYY22ccXX33d\u{2026}";
    let (out, fired, n) = mask(truncated);
    assert_eq!(n, 1, "a truncated key escaped masking: {out}");
    assert!(fired.contains("google"), "reported {fired:?}");
    assert!(!out.contains("TLCANARY"), "canary survived: {out}");
    // The nearest false positive stays clear by a wide margin.
    assert_eq!(mask("Kaizala-AIza-mock").2, 0);
}

/* ── the PEM defect ─────────────────────────────────────────────────── */

#[test]
fn a_pem_block_is_masked_whole_not_just_its_banner() {
    // Shape-derived: a realistic armoured block, TLCANARY body.
    let pem = "-----BEGIN RSA PRIVATE KEY-----\n\
               MIIEowIBAAKCAQEATLCANARYaaZZ11bbYY22ccXX33ddWW44eeVV55ffUU66gg77\n\
               hhII88jjKK99llMM00nnOOppQQrrSSttUUvvWWxxYYzz0011223344556677889\n\
               -----END RSA PRIVATE KEY-----";
    let (out, fired, n) = mask(pem);
    assert!(fired.contains("private_key_block") && n >= 1);
    assert!(
        !out.contains("MIIEowIBAAKCAQEA") && !out.contains("hhII88jjKK99llMM"),
        "the key material shipped while the treatment claimed a mask:\n{out}"
    );
    assert!(!out.contains("TLCANARY"), "canary survived: {out}");
    assert!(
        !out.contains("-----END"),
        "the closing banner should be inside the masked span: {out}"
    );
    // An UNTERMINATED block masks to the end: a truncated key is still a key.
    // A real first payload line (RFC 7468 wraps at 64). A body cut below
    // the 40-character payload bar is a NAMED residual, recorded in
    // redact.rs's measured misses rather than asserted away here.
    let truncated =
        "-----BEGIN OPENSSH PRIVATE KEY-----\nMIIEowIBAAKCAQEATLCANARYaaZZ11bbYY22ccXX33ddWW44eeVV55ffUU66gg77\n";
    let (out, _, n) = mask(truncated);
    assert!(
        n >= 1 && !out.contains("TLCANARY"),
        "unterminated block leaked: {out}"
    );
    // Prose that merely contains dashes is untouched.
    assert_eq!(mask("--- front matter --- and a ----- markdown rule").2, 0);
}

/// B7-9's two-arm proof: the sweep must see BOTH the one-line form and the
/// rustfmt-split form. Matching per line saw only the first, so the census
/// was blind to `candidates.rs::extract_commit_message` — one of the four
/// sites `TRUNCATION_SITES` itself enumerates.
#[test]
fn the_truncation_census_sees_the_rustfmt_split_form() {
    let one_line = derive_truncation_sites(Some((
        "operations.rs",
        "pub fn inj_one_line(s: &str) -> String { s.chars().take(9).collect() }\n",
    )));
    assert!(
        one_line.iter().any(|(_, n)| n == "inj_one_line"),
        "the census missed the single-line form"
    );
    let split = derive_truncation_sites(Some((
        "operations.rs",
        "pub fn inj_split(s: &str) -> String {\n    s\n        .chars()\n        .take(9)\n        .collect()\n}\n",
    )));
    assert!(
        split.iter().any(|(_, n)| n == "inj_split"),
        "the census missed the rustfmt-split form — the exact shape that hid \
         one of TRUNCATION_SITES' own entries"
    );
}
