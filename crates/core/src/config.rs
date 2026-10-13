//! `~/.thruline/config.toml`. Thresholds live here so calibration is a config
//! change, not a recompile.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub model: ModelConfig,
    pub language: LanguageConfig,
    pub thresholds: Thresholds,
    pub backward: BackwardConfig,
    pub forward: ForwardConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct LanguageConfig {
    /// Optional local language tier. When disabled or unavailable, annotation
    /// continues with the deterministic extractor and bundled NLI scorer.
    pub enabled: bool,
    /// Ollama's OpenAI-compatible endpoint. The core rejects non-loopback URLs.
    pub base_url: String,
    /// Explicit model override. `None` selects the RAM tier at startup.
    pub model: Option<String>,
    pub confidence_threshold: f32,
    pub keep_alive: String,
    /// Pause/resume hysteresis for the classification queue.
    pub pause_below_available_mb: u64,
    pub resume_above_available_mb: u64,
    /// **How long the classification queue may stay paused before it gives up.**
    ///
    /// The pause loop used to have no bound at all: `while Paused { sleep;
    /// resample }`. If available memory never rose back above
    /// `resume_above_available_mb` it waited forever — and two processes
    /// building this workspace is an ordinary CI condition, not an exotic one.
    /// A full workspace test run was observed blocked here for 13.5 minutes on
    /// 7.7 seconds of CPU, and it would have waited indefinitely.
    ///
    /// Every other wait in this product has a deadline — the Stop gate's 5 s
    /// watchdog, the hooks' 3.5 s self-timeout — and this is that rule applied
    /// here. On expiry the candidate becomes `Uncertain` with a stated reason,
    /// which is the path `enabled: false` already documents for an unavailable
    /// language tier; it is never a guessed category and never a hang.
    ///
    /// Two minutes: long enough to ride out a parallel `cargo build`, short
    /// enough that a stuck run is a failed run rather than a hung one.
    pub memory_pause_timeout_secs: u64,
    pub request_timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ModelConfig {
    /// Directory holding `config.json`, `tokenizer.json`, `model_quantized.onnx`.
    pub dir: PathBuf,
    /// Working sequence cap, ≤ `nli::MAX_TOKENS`. Set from the M3.2b probe
    /// (A25), never guessed: a cap below real premise lengths silently drops
    /// the sentence a claim needed to be entailed by.
    pub max_tokens: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Thresholds {
    /// `entailment >= hi` → supported.
    pub hi: f32,
    /// `entailment <= lo` → unsupported. Between the two → uncertain, which
    /// becomes `unclassified`, never a guess.
    pub lo: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct BackwardConfig {
    /// Candidate window: action clusters within ±k events of the claim.
    pub k_events: usize,
    /// Hard cap on candidates scored per claim.
    pub max_candidates: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ForwardConfig {
    /// Cross-encoder cap after the cheap lexical gate.
    pub max_candidates: usize,
    /// A pending node is only eligible once the project has moved on: its
    /// session is not the chronologically latest, or at least this many later
    /// user turns exist.
    pub min_subsequent_user_turns: usize,
    /// Minimum shared-token overlap for the lexical gate to admit a candidate.
    pub min_lexical_overlap: usize,
}

impl Default for ModelConfig {
    fn default() -> Self {
        ModelConfig {
            dir: crate::nli::default_model_dir().unwrap_or_else(|_| PathBuf::from("models")),
            max_tokens: crate::nli::MAX_TOKENS,
        }
    }
}

impl Default for LanguageConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            base_url: "http://127.0.0.1:11434/v1".to_string(),
            model: None,
            confidence_threshold: 0.80,
            keep_alive: "2m".to_string(),
            pause_below_available_mb: 512,
            resume_above_available_mb: 768,
            memory_pause_timeout_secs: 120,
            request_timeout_secs: 120,
        }
    }
}

impl Default for Thresholds {
    fn default() -> Self {
        // Provisional. M3.6 calibrates these against the synthetic corpus and
        // rewrites them here; until then they are deliberately conservative on
        // the side that matters — a high `hi` costs recall, a low `lo` costs
        // false alarms, and false alarms are the failure that discredits the
        // product.
        Thresholds { hi: 0.60, lo: 0.10 }
    }
}

impl Default for BackwardConfig {
    fn default() -> Self {
        BackwardConfig {
            k_events: 40,
            max_candidates: 16,
        }
    }
}

impl Default for ForwardConfig {
    fn default() -> Self {
        ForwardConfig {
            max_candidates: 32,
            min_subsequent_user_turns: 10,
            min_lexical_overlap: 1,
        }
    }
}

impl Config {
    pub fn default_path() -> Result<PathBuf> {
        match std::env::var_os("HOME") {
            Some(h) => Ok(PathBuf::from(h).join(".thruline/config.toml")),
            None => Err(CoreError::Config("HOME is not set".to_string())),
        }
    }

    /// Load, or fall back to defaults when absent. A malformed file is an
    /// error, never silently replaced by defaults — the thresholds decide what
    /// gets flagged, so quietly ignoring them would be a lie about how the
    /// verdicts were produced.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Config::default());
        }
        let text = std::fs::read_to_string(path).map_err(|e| CoreError::io(path, e))?;
        toml::from_str(&text).map_err(|e| CoreError::Config(format!("{}: {e}", path.display())))
    }

    pub fn load_default() -> Result<Self> {
        Self::load(&Self::default_path()?)
    }

    pub fn write(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| CoreError::io(dir, e))?;
        }
        let text = toml::to_string_pretty(self)
            .map_err(|e| CoreError::Config(format!("serialising config: {e}")))?;
        std::fs::write(path, text).map_err(|e| CoreError::io(path, e))
    }

    pub fn validate(&self) -> Result<()> {
        let t = self.thresholds;
        if !(0.0..=1.0).contains(&t.hi) || !(0.0..=1.0).contains(&t.lo) {
            return Err(CoreError::Config(
                "thresholds must be probabilities in [0,1]".to_string(),
            ));
        }
        if t.lo > t.hi {
            return Err(CoreError::Config(format!(
                "lo ({}) must not exceed hi ({}) — the uncertain band would be inverted",
                t.lo, t.hi
            )));
        }
        if self.model.max_tokens == 0 || self.model.max_tokens > crate::nli::MAX_TOKENS {
            return Err(CoreError::Config(format!(
                "model.max_tokens {} is outside 1..={}",
                self.model.max_tokens,
                crate::nli::MAX_TOKENS
            )));
        }
        let language = &self.language;
        if !(0.0..=1.0).contains(&language.confidence_threshold) {
            return Err(CoreError::Config(
                "language.confidence_threshold must be in [0,1]".to_string(),
            ));
        }
        let loopback = language.base_url.starts_with("http://127.0.0.1:")
            || language.base_url.starts_with("http://localhost:")
            || language.base_url.starts_with("http://[::1]:");
        if !loopback {
            return Err(CoreError::Config(format!(
                "language.base_url must be a loopback HTTP endpoint, got {}",
                language.base_url
            )));
        }
        if language.keep_alive.trim().is_empty() {
            return Err(CoreError::Config(
                "language.keep_alive must not be empty".to_string(),
            ));
        }
        // A zero timeout is an unbounded wait wearing a bound's name.
        if language.memory_pause_timeout_secs == 0 {
            return Err(CoreError::Config(
                "language.memory_pause_timeout_secs must be greater than zero; a zero \
                 deadline is the unbounded wait this field exists to remove"
                    .to_string(),
            ));
        }
        if language.pause_below_available_mb == 0
            || language.resume_above_available_mb <= language.pause_below_available_mb
        {
            return Err(CoreError::Config(
                "language memory thresholds require 0 < pause < resume".to_string(),
            ));
        }
        if !(1..=600).contains(&language.request_timeout_secs) {
            return Err(CoreError::Config(
                "language.request_timeout_secs must be in 1..=600".to_string(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_round_trip_through_toml() {
        let c = Config::default();
        let text = toml::to_string_pretty(&c).expect("serialise");
        let back: Config = toml::from_str(&text).expect("parse");
        assert_eq!(c, back);
    }

    #[test]
    fn a_partial_file_keeps_defaults_for_the_rest() {
        let c: Config = toml::from_str("[thresholds]\nhi = 0.8\n").expect("parse");
        assert_eq!(c.thresholds.hi, 0.8);
        assert_eq!(c.thresholds.lo, Thresholds::default().lo);
        assert_eq!(c.forward.max_candidates, 32);
        assert_eq!(c.language, LanguageConfig::default());
    }

    #[test]
    fn an_inverted_band_is_rejected() {
        let c = Config {
            thresholds: Thresholds { hi: 0.2, lo: 0.8 },
            ..Default::default()
        };
        assert!(c.validate().is_err());
    }

    #[test]
    fn a_malformed_file_is_an_error_not_a_silent_default() {
        let dir = tempfile::tempdir().expect("tempdir");
        let p = dir.path().join("config.toml");
        std::fs::write(&p, "this is not toml {{{").expect("write");
        assert!(
            Config::load(&p).is_err(),
            "silently defaulting would misreport how verdicts were produced"
        );
    }

    #[test]
    fn a_missing_file_is_fine() {
        let dir = tempfile::tempdir().expect("tempdir");
        let c = Config::load(&dir.path().join("nope.toml")).expect("defaults");
        assert_eq!(c, Config::default());
    }

    #[test]
    fn language_tier_is_loopback_only_and_uses_hysteresis() {
        let mut c = Config::default();
        c.language.base_url = "https://example.com/v1".to_string();
        assert!(c.validate().is_err(), "classification must stay local");

        c.language.base_url = "http://localhost:11434/v1".to_string();
        c.language.resume_above_available_mb = c.language.pause_below_available_mb;
        assert!(c.validate().is_err(), "pause/resume needs hysteresis");
    }
}
