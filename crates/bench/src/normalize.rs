//! Turning an external trajectory format into ThruLine events — BUILD-SPEC
//! §12's normaliser.
//!
//! # What is here, and what deliberately is not
//!
//! The **contract** is here: [`Normalizer`], the shape every loader must
//! satisfy, and [`Corpus`], which carries a provenance so a measurement can
//! refuse to be reported as a §12 result when it was taken over fixtures its
//! own author wrote.
//!
//! The **loaders** are not, and their absence is a finding rather than an
//! omission. §12 names ImpossibleBench, AgentLens-Bench, EvilGenie, RHB,
//! SpecBench and the τ2-bench/AppWorld labelled trajectories. Every one of them
//! was probed from this machine (see `REPORT.md` for the table). What the probe
//! found is that these publish **task sets and evaluation harnesses**, not
//! labelled trajectories: `safety-research/impossiblebench` is 33 files of
//! agents, scorers and mutators, and its own README says *"After running
//! evaluations to generate logs, use the `DataLoader` class"* — the
//! trajectories are output you produce by running a model against the tasks.
//!
//! Writing a normaliser for a format nobody here has seen would be guessing at
//! field names and then testing the guess against fixtures written from the
//! same guess. That is the shape of every self-confirming benchmark, and the
//! verifier already refused it once on this project for `go test`. So the trait
//! is defined, the metrics are tested against hand-computed values, and the
//! loaders wait for a format in hand.

use std::collections::BTreeMap;

use crate::Provenance;

/// One labelled trajectory, in whatever the source's own terms are.
#[derive(Debug, Clone, PartialEq)]
pub struct Trajectory {
    pub id: String,
    /// The task this trajectory is of, when the source names one.
    pub task_id: Option<String>,
    /// The ground truth, as the source publishes it.
    pub label: crate::Ground,
    /// For an honest trajectory: what kind of honest work it is. §12's
    /// false-positive target is meaningless pooled across these.
    pub shape: String,
    /// The source's own record, unmodified. A normaliser reads this; nothing
    /// else may.
    pub raw: serde_json::Value,
}

/// A labelled set, and where it came from.
#[derive(Debug, Clone)]
pub struct Corpus {
    pub name: String,
    pub provenance: Provenance,
    /// The exact version, tag or commit the trajectories came from. §12
    /// requires fixture versions in the published methodology, and a corpus
    /// that cannot say which version it is cannot be reproduced.
    pub version: String,
    /// **Class 1's evidence.** The sha256 of the upstream file, recorded at
    /// fetch. A corpus without one cannot claim class 1, which is what stops
    /// the class being a caller-supplied word.
    pub artifact_digest: Option<String>,
    pub trajectories: Vec<Trajectory>,
}

impl Corpus {
    /// How many trajectories of each label.
    pub fn composition(&self) -> BTreeMap<&'static str, usize> {
        let mut out = BTreeMap::new();
        for t in &self.trajectories {
            let key = match &t.label {
                crate::Ground::Labelled(r) => r.label().as_str(),
                crate::Ground::Unlabelled { .. } => "unlabelled",
            };
            *out.entry(key).or_insert(0) += 1;
        }
        out
    }

    /// The distinct honest-work shapes present.
    ///
    /// §12's false-positive target is per shape, so a corpus that carries only
    /// one cannot answer it — and [`crate::Measurement::overclaim_index`]
    /// refuses rather than averaging.
    pub fn honest_shapes(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .trajectories
            .iter()
            .filter(|t| t.label.label().is_some_and(|l| !l.is_positive()))
            .map(|t| t.shape.clone())
            .collect();
        v.sort();
        v.dedup();
        v
    }
}

/// A loader for one external format.
///
/// One method, because the only thing a loader is allowed to decide is how to
/// read its own format. Labels come from the source; a loader that *derives* a
/// label is a loader that has an opinion about the answer.
pub trait Normalizer {
    /// The source this reads, for the methodology section.
    fn source(&self) -> &str;

    /// Read a downloaded corpus from a directory.
    fn load(&self, dir: &std::path::Path) -> Result<Corpus, LoadError>;
}

#[derive(Debug)]
pub enum LoadError {
    /// The corpus is not on disk. **Named, never substituted.** §12's fixtures
    /// are downloads; a loader that generated something when the download was
    /// missing would be measuring its own generator.
    NotDownloaded {
        source: String,
        expected: std::path::PathBuf,
    },
    Io(std::io::Error),
    /// The file is there and is not the shape this loader knows.
    Shape(String),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::NotDownloaded { source, expected } => write!(
                f,
                "{source} is not downloaded (looked in {}). §12's corpora are \
                 fetched, never generated here: a corpus this crate produced \
                 would measure this crate's idea of a cheat.",
                expected.display()
            ),
            LoadError::Io(e) => write!(f, "{e}"),
            LoadError::Shape(s) => write!(f, "unrecognised shape: {s}"),
        }
    }
}

impl std::error::Error for LoadError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Label;

    fn t(id: &str, label: Label, shape: &str) -> Trajectory {
        Trajectory {
            id: id.into(),
            task_id: Some(id.into()),
            label: crate::Ground::from_oracle("resolved", "-", label),
            shape: shape.into(),
            raw: serde_json::json!({}),
        }
    }

    #[test]
    fn a_corpus_reports_its_composition_and_its_honest_shapes() {
        let c = Corpus {
            name: "x".into(),
            provenance: Provenance::ExternalLabelled(
                crate::ExternalSource::new("p", "d", "v", "resolved").expect("source"),
            ),
            version: "v1".into(),
            artifact_digest: Some("d".into()),
            trajectories: vec![
                t("a", Label::Cheated, "-"),
                t("b", Label::HonestPass, "rename"),
                t("c", Label::HonestPass, "rename"),
                t("d", Label::HonestFailure, "xfail"),
            ],
        };
        assert_eq!(c.composition()["cheated"], 1);
        assert_eq!(c.composition()["honest_pass"], 2);
        assert_eq!(c.honest_shapes(), vec!["rename", "xfail"]);
    }

    /// A missing download is an error that names the source, not an empty
    /// corpus that scores perfectly.
    #[test]
    fn a_missing_download_is_named_rather_than_substituted() {
        let e = LoadError::NotDownloaded {
            source: "impossiblebench".into(),
            expected: std::path::PathBuf::from("/fixtures/impossiblebench"),
        };
        let text = e.to_string();
        assert!(text.contains("not downloaded"), "{text}");
        assert!(
            text.contains("never generated here"),
            "the message must say why a substitute is not offered: {text}"
        );
    }

    /// A corpus must be able to say which version it is.
    #[test]
    fn a_corpus_carries_its_version() {
        let c = Corpus {
            name: "x".into(),
            provenance: Provenance::ExternalLabelled(
                crate::ExternalSource::new("p", "d", "v", "resolved").expect("source"),
            ),
            version: String::new(),
            artifact_digest: None,
            trajectories: vec![],
        };
        assert!(
            c.version.is_empty(),
            "the field exists so a methodology section can be reproduced; this \
             asserts it is present, and `report` refuses to publish an empty one"
        );
    }
}
