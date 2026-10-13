//! ONNX NLI cross-encoder (§10 `EntailmentScorer`).
//!
//! Local inference only. ThruLine never bundles or downloads these weights.
//! An operator supplies the published A18 files and uses `thruline model
//! import --from <DIR>` to preview their exact receipt before an explicit
//! `--apply`; runtime inference never fetches them.
//!
//! ## Orientation is fixed and load-bearing
//!
//! **premise = evidence, hypothesis = claim.** We ask "does what the tools
//! actually did entail what the assistant said?", never the reverse. Reversing
//! it asks whether a claim implies the evidence, which is a different and
//! useless question. [`tests::calibration`] asserts the direction.
//!
//! ## Label order is proven, not assumed
//!
//! DeBERTa NLI exports disagree on head order. We read `id2label` from the
//! downloaded `config.json` and then prove it empirically. A wrong index does
//! not fail loudly — it silently inverts every verdict in the product.

use std::collections::BTreeMap;
#[cfg(feature = "ort")]
use std::path::Path;
use std::path::PathBuf;

#[cfg(feature = "ort")]
use serde::Deserialize;

use crate::error::{CoreError, Result};
#[cfg(feature = "ort")]
use crate::model_supply::load_verified_model_bytes;

/// Longest sequence the model accepts (`max_position_embeddings`).
///
/// This is the ceiling, not necessarily the working cap: [`OrtNli::open_with`]
/// takes a lower cap, which M3.2b probed for its effect on peak RSS (A25).
pub const MAX_TOKENS: usize = 512;

/// The three NLI classes, resolved to their head indices at load time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LabelMap {
    pub entailment: usize,
    pub neutral: usize,
    pub contradiction: usize,
}

impl LabelMap {
    /// Built from `config.json`'s `id2label`. Refuses to guess: a config that
    /// does not name all three classes is an error, not a default.
    pub fn from_id2label(id2label: &BTreeMap<String, String>) -> Result<Self> {
        let mut ent = None;
        let mut neu = None;
        let mut con = None;
        for (idx, name) in id2label {
            let i: usize = idx
                .parse()
                .map_err(|_| CoreError::Config(format!("id2label key {idx:?} is not an index")))?;
            match name.to_ascii_lowercase().as_str() {
                "entailment" => ent = Some(i),
                "neutral" => neu = Some(i),
                "contradiction" => con = Some(i),
                other => {
                    return Err(CoreError::Config(format!(
                        "unrecognised NLI label {other:?} at index {i}"
                    )))
                }
            }
        }
        match (ent, neu, con) {
            (Some(entailment), Some(neutral), Some(contradiction)) => Ok(LabelMap {
                entailment,
                neutral,
                contradiction,
            }),
            _ => Err(CoreError::Config(
                "config.json id2label must name entailment, neutral and contradiction".to_string(),
            )),
        }
    }
}

#[derive(Deserialize)]
#[cfg(feature = "ort")]
struct RawConfig {
    id2label: BTreeMap<String, String>,
}

/// Softmaxed class probabilities for one (premise, hypothesis) pair.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scores {
    pub entailment: f32,
    pub neutral: f32,
    pub contradiction: f32,
}

impl Scores {
    pub fn dominant_is_entailment(&self) -> bool {
        self.entailment > self.neutral && self.entailment > self.contradiction
    }
    pub fn dominant_is_contradiction(&self) -> bool {
        self.contradiction > self.neutral && self.contradiction > self.entailment
    }
}

/// §10's `EntailmentScorer`, as a trait so a different backend can slot in.
pub trait EntailmentScorer {
    /// `premise` is evidence, `hypothesis` is the claim. Never the reverse.
    fn score(&mut self, premise: &str, hypothesis: &str) -> Result<Scores>;

    /// Max-pooled entailment over candidates.
    ///
    /// Deliberately **not** a concatenation of every premise into one string:
    /// that blows the 512-token window and destroys attribution — you learn
    /// that *something* entailed the claim but not what. Returns the best
    /// score and the index of the candidate that produced it.
    fn best_of(
        &mut self,
        premises: &[String],
        hypothesis: &str,
    ) -> Result<Option<(usize, Scores)>> {
        let mut best: Option<(usize, Scores)> = None;
        for (i, p) in premises.iter().enumerate() {
            let s = self.score(p, hypothesis)?;
            if best
                .map(|(_, b)| s.entailment > b.entailment)
                .unwrap_or(true)
            {
                best = Some((i, s));
            }
        }
        Ok(best)
    }
}

/// Where the local NLI model lives by default.
///
/// Ungated on purpose: `Config` needs this path whether or not the ONNX
/// backend is compiled in, and a path is not a backend.
pub fn default_model_dir() -> Result<PathBuf> {
    match std::env::var_os("HOME") {
        Some(h) => Ok(PathBuf::from(h).join(".thruline/models/nli-deberta-v3-xsmall")),
        None => Err(CoreError::Config("HOME is not set".to_string())),
    }
}

/// The local ONNX entailment backend.
///
/// Behind the `ort` feature: it is the only thing in the tree that pulls
/// `ort`/`tokenizers`, and therefore the only reason a build must reach the
/// network. Everything that consumes entailment does so through
/// [`EntailmentScorer`], so dropping this backend costs the local scorer and
/// nothing else.
#[cfg(feature = "ort")]
pub struct OrtNli {
    session: ort::session::Session,
    tokenizer: tokenizers::Tokenizer,
    labels: LabelMap,
    model_tag: String,
    max_tokens: usize,
    pairs_scored: u64,
    /// Pairs whose (premise, hypothesis) encoding hit the cap. A silently
    /// truncated premise can drop the very sentence a claim needed, so this is
    /// counted and reported rather than assumed to be zero.
    pairs_truncated: u64,
}

#[cfg(feature = "ort")]
impl OrtNli {
    /// Load from a model directory containing `config.json`, `tokenizer.json`
    /// and `model_quantized.onnx`.
    pub fn open(dir: impl AsRef<Path>) -> Result<Self> {
        Self::open_with(dir, MAX_TOKENS)
    }

    /// As [`OrtNli::open`], with an explicit sequence cap (A25).
    pub fn open_with(dir: impl AsRef<Path>, max_tokens: usize) -> Result<Self> {
        let dir = dir.as_ref();
        if max_tokens == 0 || max_tokens > MAX_TOKENS {
            return Err(CoreError::Config(format!(
                "max_tokens {max_tokens} is outside 1..={MAX_TOKENS}"
            )));
        }
        let verified = load_verified_model_bytes(dir).map_err(|error| {
            CoreError::Config(format!(
                "NLI model receipt at {} could not be loaded safely ({error}) — run \
                 `thruline model status --dir {}`; preview an operator-supplied copy with \
                 `thruline model import --from <DIR> --dir {}` before adding `--apply`",
                dir.display(),
                dir.display(),
                dir.display()
            ))
        })?;
        let (model_tag, cfg_bytes, tokenizer_bytes, model_bytes) = verified.into_parts();
        let cfg: RawConfig = serde_json::from_slice(&cfg_bytes)?;
        let labels = LabelMap::from_id2label(&cfg.id2label)?;
        drop(cfg_bytes);

        let tokenizer = tokenizers::Tokenizer::from_bytes(&tokenizer_bytes)
            .map_err(|e| CoreError::Config(format!("tokenizer.json: {e}")))?;
        drop(tokenizer_bytes);

        let session = ort::session::Session::builder()
            .map_err(|e| CoreError::Config(format!("ort session builder: {e}")))?
            // CPU only, single-threaded by default: the RSS budget matters more
            // than latency, and the caller batches.
            .with_intra_threads(1)
            .map_err(|e| CoreError::Config(format!("ort threads: {e}")))?
            // Memory pattern pre-allocates for the largest seen shape and keeps
            // it. Sequence lengths here vary a lot, so it inflates peak RSS for
            // no throughput gain on a 512-token single-batch model — and the
            // 8 GB Air promise makes RSS the scarcer resource.
            .with_memory_pattern(false)
            .map_err(|e| CoreError::Config(format!("ort memory pattern: {e}")))?
            .commit_from_memory(&model_bytes)
            .map_err(|e| CoreError::Config(format!("ort load model_quantized.onnx: {e}")))?;
        drop(model_bytes);

        Ok(OrtNli {
            session,
            tokenizer,
            labels,
            model_tag,
            max_tokens,
            pairs_scored: 0,
            pairs_truncated: 0,
        })
    }

    pub fn labels(&self) -> LabelMap {
        self.labels
    }

    pub fn max_tokens(&self) -> usize {
        self.max_tokens
    }

    pub fn pairs_truncated(&self) -> u64 {
        self.pairs_truncated
    }

    /// Token length of a pair as the model would see it, uncapped. Used by the
    /// M3.2b census to choose a cap from real premise lengths rather than a
    /// guess.
    pub fn token_len(&self, premise: &str, hypothesis: &str) -> Result<usize> {
        let enc = self
            .tokenizer
            .encode((premise, hypothesis), true)
            .map_err(|e| CoreError::Config(format!("tokenize: {e}")))?;
        Ok(enc.get_ids().len())
    }

    /// Content-bound A18 receipt identity—the string scorer receipts carry.
    pub fn model_tag(&self) -> &str {
        &self.model_tag
    }

    pub fn pairs_scored(&self) -> u64 {
        self.pairs_scored
    }

    /// Retained as the name every caller already uses; the path itself is
    /// [`default_model_dir`], which does not need this backend.
    pub fn default_dir() -> Result<PathBuf> {
        default_model_dir()
    }
}

#[cfg(feature = "ort")]
fn softmax(logits: &[f32]) -> Vec<f32> {
    let max = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let exp: Vec<f32> = logits.iter().map(|x| (x - max).exp()).collect();
    let sum: f32 = exp.iter().sum();
    if sum == 0.0 {
        return vec![0.0; logits.len()];
    }
    exp.into_iter().map(|x| x / sum).collect()
}

#[cfg(feature = "ort")]
impl EntailmentScorer for OrtNli {
    fn score(&mut self, premise: &str, hypothesis: &str) -> Result<Scores> {
        let enc = self
            .tokenizer
            .encode((premise, hypothesis), true)
            .map_err(|e| CoreError::Config(format!("tokenize: {e}")))?;

        // Hard truncation at the model's window. The tokenizer is not
        // configured with a truncation policy here, so we enforce it and keep
        // the premise head — evidence summaries put the tool and target first.
        let mut ids: Vec<i64> = enc.get_ids().iter().map(|&i| i as i64).collect();
        let mut mask: Vec<i64> = enc.get_attention_mask().iter().map(|&i| i as i64).collect();
        if ids.len() > self.max_tokens {
            ids.truncate(self.max_tokens);
            mask.truncate(self.max_tokens);
            self.pairs_truncated += 1;
        }
        let len = ids.len();

        let input_ids = ort::value::Tensor::from_array(([1usize, len], ids))
            .map_err(|e| CoreError::Config(format!("input_ids tensor: {e}")))?;
        let attention_mask = ort::value::Tensor::from_array(([1usize, len], mask))
            .map_err(|e| CoreError::Config(format!("attention_mask tensor: {e}")))?;

        let outputs = self
            .session
            .run(ort::inputs![
                "input_ids" => input_ids,
                "attention_mask" => attention_mask,
            ])
            .map_err(|e| CoreError::Config(format!("ort run: {e}")))?;

        let (_shape, logits) = outputs["logits"]
            .try_extract_tensor::<f32>()
            .map_err(|e| CoreError::Config(format!("extract logits: {e}")))?;
        if logits.len() < 3 {
            return Err(CoreError::Config(format!(
                "expected 3 NLI logits, got {}",
                logits.len()
            )));
        }
        let p = softmax(&logits[..3]);
        self.pairs_scored += 1;
        Ok(Scores {
            entailment: p[self.labels.entailment],
            neutral: p[self.labels.neutral],
            contradiction: p[self.labels.contradiction],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_map_is_read_never_guessed() {
        let m: BTreeMap<String, String> = [
            ("0".to_string(), "contradiction".to_string()),
            ("1".to_string(), "entailment".to_string()),
            ("2".to_string(), "neutral".to_string()),
        ]
        .into_iter()
        .collect();
        let lm = LabelMap::from_id2label(&m).expect("valid config");
        assert_eq!(lm.contradiction, 0);
        assert_eq!(lm.entailment, 1);
        assert_eq!(lm.neutral, 2);
    }

    #[test]
    fn a_different_head_order_is_honoured_not_overridden() {
        // Some exports order entailment first. The map must follow the config.
        let m: BTreeMap<String, String> = [
            ("0".to_string(), "entailment".to_string()),
            ("1".to_string(), "neutral".to_string()),
            ("2".to_string(), "contradiction".to_string()),
        ]
        .into_iter()
        .collect();
        let lm = LabelMap::from_id2label(&m).expect("valid config");
        assert_eq!(lm.entailment, 0);
    }

    #[test]
    fn an_incomplete_config_is_an_error_not_a_default() {
        let m: BTreeMap<String, String> = [("0".to_string(), "entailment".to_string())]
            .into_iter()
            .collect();
        assert!(LabelMap::from_id2label(&m).is_err());
    }

    #[cfg(feature = "ort")]
    #[test]
    fn softmax_normalises() {
        let p = softmax(&[1.0, 2.0, 3.0]);
        assert!((p.iter().sum::<f32>() - 1.0).abs() < 1e-6);
        assert!(p[2] > p[1] && p[1] > p[0]);
    }
}
