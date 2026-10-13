//! Language-tier checks that need no private data.
//!
//! The original M4.3 measurement against the hand-labelled A19 yardstick
//! resolved private transcript text from a local ledger. The yardstick rows
//! are not part of this public copy, so that test is not here; what remains
//! is the Round-7 pattern set, which is a handful of invented sentences.

use thruline_core::annotate::language_candidate_message;
use thruline_core::extract::sentence_candidates;
use thruline_core::language::{
    Annotator, ClassificationOutcome, LanguageCategory, OllamaAnnotator, OllamaOptions,
};

struct Round7Pattern {
    name: &'static str,
    sentence: &'static str,
    accepted: &'static [LanguageCategory],
}

const PROCEDURAL_SAFE: &[LanguageCategory] = &[
    LanguageCategory::Procedure,
    LanguageCategory::Guidance,
    LanguageCategory::Uncertain,
];
const FINDING_OR_UNCERTAIN: &[LanguageCategory] =
    &[LanguageCategory::Finding, LanguageCategory::Uncertain];
const FINDING_ONLY: &[LanguageCategory] = &[LanguageCategory::Finding];
const ROUND7_PATTERNS: [Round7Pattern; 7] = [
    Round7Pattern {
        name: "future-procedure",
        sentence: "After the migration, the verifier will check every token.",
        accepted: PROCEDURAL_SAFE,
    },
    Round7Pattern {
        name: "numbered-future-procedure",
        sentence: "Next: rebuild the index, verify eighteen rows, then deploy.",
        accepted: PROCEDURAL_SAFE,
    },
    Round7Pattern {
        name: "bare-imperative",
        sentence: "Rebuild the cache and verify all eighteen entries.",
        accepted: PROCEDURAL_SAFE,
    },
    Round7Pattern {
        name: "second-person-imperative",
        sentence: "You should rotate the key before deploying.",
        accepted: PROCEDURAL_SAFE,
    },
    Round7Pattern {
        name: "conditional-guidance",
        sentence: "If the checksum changes, rerun the migration.",
        accepted: PROCEDURAL_SAFE,
    },
    Round7Pattern {
        name: "uncertain-diagnosis",
        sentence: "The timeout may be caused by allocator pressure.",
        accepted: FINDING_OR_UNCERTAIN,
    },
    Round7Pattern {
        name: "causal-finding",
        sentence: "The slowdown comes from repeated tokenizer allocation.",
        accepted: FINDING_ONLY,
    },
];

fn classified_category(
    provider: &dyn Annotator,
    message: &str,
    threshold: f32,
) -> (LanguageCategory, u8) {
    match provider.classify(message) {
        ClassificationOutcome::Classified { output, attempts } => {
            let category = if output.confidence.get() >= threshold {
                output.category
            } else {
                LanguageCategory::Uncertain
            };
            (category, attempts)
        }
        ClassificationOutcome::Uncertain { attempts, .. } => {
            (LanguageCategory::Uncertain, attempts)
        }
    }
}

#[test]
#[ignore = "requires a local THRULINE_LANGUAGE_MODEL"]
fn round7_language_patterns() {
    let model = std::env::var("THRULINE_LANGUAGE_MODEL").expect("THRULINE_LANGUAGE_MODEL");
    let provider = OllamaAnnotator::new(OllamaOptions {
        model_override: Some(model.clone()),
        ..OllamaOptions::default()
    })
    .expect("local provider");
    let mut correct = 0usize;
    for pattern in &ROUND7_PATTERNS {
        let candidates = sentence_candidates(pattern.sentence);
        let (category, _) =
            classified_category(&provider, &language_candidate_message(&candidates, 0), 0.80);
        let pass = pattern.accepted.contains(&category);
        correct += usize::from(pass);
        println!(
            "ROUND7_LANGUAGE pattern={} category={} correct={pass}",
            pattern.name,
            category.as_str()
        );
    }
    println!("ROUND7_LANGUAGE model={model} correct={correct}/7");
}

#[test]
fn round7_regression_grammars_are_named_and_conviction_safe() {
    let names: std::collections::BTreeSet<&str> =
        ROUND7_PATTERNS.iter().map(|pattern| pattern.name).collect();
    assert_eq!(names.len(), 7);
    for pattern in &ROUND7_PATTERNS {
        assert!(!sentence_candidates(pattern.sentence).is_empty());
        assert!(!pattern.accepted.iter().any(|category| matches!(
            category,
            LanguageCategory::CompletedClaim | LanguageCategory::Proposal
        )));
    }
}
