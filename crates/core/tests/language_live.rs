//! Manual localhost-only throughput probe for M4.2. Never runs by default.

use std::time::Instant;

use thruline_core::language::{
    Annotator, ClassificationOutcome, OllamaAnnotator, OllamaOptions, ServiceAvailability,
};

#[test]
#[ignore = "requires a running local Ollama service and THRULINE_LANGUAGE_MODEL"]
fn local_ollama_classification_probe() {
    let model = std::env::var("THRULINE_LANGUAGE_MODEL")
        .expect("set THRULINE_LANGUAGE_MODEL to an already-installed model");
    let provider = OllamaAnnotator::new(OllamaOptions {
        model_override: Some(model.clone()),
        ..OllamaOptions::default()
    })
    .expect("provider");
    assert!(
        matches!(
            provider.probe(),
            ServiceAvailability::Available {
                selected_model_present: true,
                ..
            }
        ),
        "probe must never download a missing model"
    );

    let sentences = [
        "All 18 parser tests pass.",
        "I will update `docs/guide.md` in the next pass.",
        "The slowdown comes from repeated tokenizer allocation.",
        "Let me inspect the query plan.",
        "You should keep the old index until rollout is complete.",
        "Run the migration, verify the row count, then deploy.",
        "Build not saved?",
        "I am checking the remaining files now.",
        "This wording could describe either a plan or a result.",
    ];
    let started = Instant::now();
    let mut attempts = 0_u64;
    let mut safe_uncertain = 0_u64;
    for sentence in sentences {
        match provider.classify(sentence) {
            ClassificationOutcome::Classified {
                output,
                attempts: used,
            } => {
                attempts += u64::from(used);
                safe_uncertain += u64::from(
                    output.category == thruline_core::language::LanguageCategory::Uncertain,
                );
                println!(
                    "{}\t{:.3}\t{}",
                    output.category.as_str(),
                    output.confidence.get(),
                    sentence
                );
            }
            ClassificationOutcome::Uncertain {
                attempts: used,
                reason,
            } => {
                panic!("provider failed closed after {used} attempts: {reason:?}");
            }
        }
    }
    let elapsed = started.elapsed().as_secs_f64();
    println!(
        "LANGUAGE_BENCH model={model} sentences={} attempts={attempts} safe_uncertain={} \
         elapsed_s={elapsed:.3} per_sec={:.3}",
        sentences.len(),
        safe_uncertain,
        sentences.len() as f64 / elapsed.max(0.001),
    );
}
