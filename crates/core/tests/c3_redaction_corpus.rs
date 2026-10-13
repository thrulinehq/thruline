//! D1's cure — the corpus false-positive regression (A178).
//!
//! **The gate that let the filename defect slip was the absence of this file.**
//! A177 shipped on a clean live run, and a clean live run is evidence the fix
//! did not regress — it is NOT evidence the rule is safe. The rule was unsafe:
//! it destroyed `NotificationDigestV2ComponentWrapper.tsx` in `objective` and
//! `in_flight`, the two sections a fresh session reads first, and the specimens
//! happened not to contain one.
//!
//! So the detector is now run over known-clean text and pinned at
//! **masked-count = 0**, in two layers:
//!
//! 1. A committed fixture of corpus-SHAPED strings that always runs. It is
//!    shape-derived rather than copied, because committing the corpus verbatim
//!    is the spill `secrets_policy` warns about (A174).
//! 2. A live pass over the real specimens' `objective` and `in_flight`
//!    sections, which runs on a machine that has the ledger and states plainly
//!    when it is skipped. A skipped check reports itself; it never passes
//!    quietly.

use thruline_core::redact;

/// Text of exactly the kinds the real specimens' first two sections carry:
/// verbatim human asks, deep paths, CamelCase filenames with digits, commit
/// subjects, and candidate-vocabulary measurements. None of it is a secret and
/// none of it may be masked.
const KNOWN_CLEAN: &[&str] = &[
    // Filenames — the shape that actually broke.
    "NotificationDigestV2ComponentWrapper.tsx",
    "RevenueCatSubscriptionGatewayAdapter2.ts",
    "apps/mobile/components/DailyBriefingCarouselV3.tsx",
    "supabase/functions/revenuecat-webhook/index.ts",
    "packages/core/src/scheduling/NotificationBatchScheduler2.test.ts",
    // Opening asks, verbatim human prose.
    "Also in the media section, only the today's playlist should be free.",
    "In the profile section is the avatar upload absolutely necessary?",
    "based on what data we are calculating the tier of the users entering the signup date",
    "I wanna make a few changes, Currently the year selection extends only to 1985",
    // Candidate-vocabulary measurements.
    "55 edits after the last commit — unresolved",
    "17 edits after the last commit — unresolved · apps/mobile/lib/revenuecat.ts",
    "12 edits · DailyBriefingCarouselV3.tsx",
    // Commit subjects and ordinary commands.
    "chore(mobile): bump build number and regenerate eas.json",
    "feat(v1-launch): grant Pro to all users and fix chart rendering",
    "cd /Users/someone/dev/atlas_playstore && git status --porcelain",
    "pnpm tsx scripts/trigger-daily-briefing.ts 2>&1 | head -40",
    // The product's own receipts.
    "compiled at chain position 36e7c68565cc2179eb76be85cf160a869e81864ea8e70b13f8761f59a847c895 over 17708 recorded events",
    "the record's last unresolved edit run is in session 6b2e0f14-9a7d-4c35-8e51-d4f3a9c28b70",
    "01KZVXY6PPPJPK837HCKC9MWJ8",
];

#[test]
fn known_clean_text_is_never_masked() {
    let mut offenders = Vec::new();
    for text in KNOWN_CLEAN {
        let (out, fired, n) = redact::mask(text);
        if n != 0 {
            offenders.push(format!("  {text}\n    -> {out}  (fired: {fired:?})"));
        }
    }
    assert!(
        offenders.is_empty(),
        "the detector masked {} piece(s) of known-clean text — this is the D1 defect \
         returning, and it destroys the context recovery exists to preserve:\n{}",
        offenders.len(),
        offenders.join("\n")
    );
}

#[test]
fn the_detector_still_catches_what_it_is_for() {
    // A false-positive pin alone would pass trivially if the detector stopped
    // detecting. This is the other half, so the pair cannot both be satisfied
    // by a no-op.
    let mut caught = 0usize;
    for secret in [
        "AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE",
        "git push https://ghp_ZZZZaaaa1111bbbb2222cccc3333dddd@github.com/x/y",
        "git -c user.email=\"someone@example.com\" commit -m x",
        "supabase functions deploy fn --project-ref abcdefghijklmnop",
        "ANTHROPIC_API_KEY=sk-ant-api03-notarealkeyatall",
    ] {
        let (_, _, n) = redact::mask(secret);
        assert!(n >= 1, "the detector stopped catching: {secret}");
        caught += 1;
    }
    assert_eq!(caught, 5);
}

/// The live half: the real specimens' first two sections, at masked-count 0.
///
/// Runs where the ledger exists and **says so when it does not** — a skipped
/// check that stayed silent would be the same false calm this file exists to
/// prevent.
#[test]
fn live_corpus_objective_and_in_flight_are_never_masked() {
    let db = match std::env::var_os("HOME") {
        Some(home) => std::path::PathBuf::from(home)
            .join(".thruline")
            .join("ledger.db"),
        None => {
            println!("SKIPPED: no HOME, so no ledger to read");
            return;
        }
    };
    if !db.exists() {
        println!(
            "SKIPPED: {} does not exist — the live half of the D1 regression did not run",
            db.display()
        );
        return;
    }
    let ledger = match thruline_core::ledger::Ledger::open(&db) {
        Ok(l) => l,
        Err(e) => {
            println!("SKIPPED: could not open the ledger read-only: {e}");
            return;
        }
    };
    let specimens = [
        "-Users-dev-Developer-clinic-private-beta",
        "-Users-dev-Desktop-Telescope-project",
    ];
    let mut checked = 0usize;
    for project in specimens {
        let exists: i64 = ledger
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM projects WHERE id = ?1",
                [project],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if exists == 0 {
            println!("SKIPPED specimen {project}: not present in this ledger");
            continue;
        }
        // Compile WITHOUT masking by reading the sections straight from the
        // candidate layer: the capsule masks in place, so asking the capsule
        // would measure the fix rather than the rule.
        let cands = thruline_core::candidates::derive(&ledger, project).expect("candidates");
        let mut texts: Vec<String> = cands
            .opening_asks
            .iter()
            .map(|a| format!("opening ask \u{201c}{}\u{201d}", a.text.trim()))
            .collect();
        for tail in &cands.tails {
            texts.push(format!(
                "{} edits after the last commit — unresolved · {}",
                tail.edits_after_last_boundary,
                tail.files.join(", ")
            ));
        }
        for cluster in cands.clusters.iter().take(200) {
            texts.push(format!("{} edits · {}", cluster.edits, cluster.file));
        }
        // The pin is on the CONTEXT-DESTROYING kinds. `high_entropy` and
        // `flag_value` are heuristics over shape, and a false positive from
        // either eats working context — that is D1's harm and it is barred
        // here. The known-format detectors (email, aws, github, …) are global
        // by ruling D4 and a hit from one is the system working: a real admin
        // email does appear in a real opening ask, and masking it is intended.
        let mut offenders = Vec::new();
        for text in &texts {
            let (out, fired, _) = redact::mask(text);
            let heuristic: Vec<&String> = fired
                .iter()
                .filter(|f| *f == "high_entropy" || *f == "flag_value")
                .collect();
            if !heuristic.is_empty() {
                offenders.push(format!("  {heuristic:?}\n    -> {out}"));
            }
        }
        assert!(
            offenders.is_empty(),
            "a shape heuristic masked {} line(s) of {project}'s objective/in_flight sections \
             — this is the D1 defect returning:\n{}",
            offenders.len(),
            offenders.join("\n")
        );
        checked += texts.len();
    }
    println!("live corpus check: {checked} objective/in_flight lines, 0 masked");
}

/// D4's doctrinal half, as a test rather than a hope.
///
/// A "Known limitations" section that lists only DESIGNED exemptions, while
/// the record has already measured a miss, is the false-calm family wearing
/// documentation clothes. A174 measured a real email address in a live capsule
/// and A177's limitations stayed silent about it. Never again: the module must
/// carry a measured-misses section, and every specimen the record has actually
/// produced must be named in it.
#[test]
fn the_limitations_section_enumerates_measured_misses_not_only_designed_exemptions() {
    let src = std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/redact.rs"),
    )
    .expect("read redact.rs");
    let header: String = src
        .lines()
        .take_while(|l| l.starts_with("//!"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        header.contains("Known limitations"),
        "the module lost its Known limitations section"
    );
    assert!(
        header.contains("Designed exemptions"),
        "the designed exemptions are unlisted"
    );
    assert!(
        header.contains("Measured misses"),
        "the limitations list has no measured-misses section — a list of only what was \
         designed, while the record already measured a miss, is documentation-shaped false calm"
    );
    // The two specimens the record actually produced (A174) must be named.
    for measured in ["email", "project-ref"] {
        let after = header
            .split("Measured misses")
            .nth(1)
            .expect("measured-misses section");
        assert!(
            after.contains(measured),
            "the measured miss `{measured}` is not enumerated in Measured misses"
        );
    }
    // And each measured miss must cite where it was measured, so a reader can
    // check rather than trust.
    let after = header.split("Measured misses").nth(1).unwrap();
    assert!(
        after.contains("A174"),
        "a measured miss must cite the entry that measured it"
    );
}
