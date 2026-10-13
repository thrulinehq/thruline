//! Optional local language classification for M4.
//!
//! This module is deliberately separate from claims, evidence and verdicts.
//! Requests contain one assistant message and classification instructions;
//! they have no evidence or verdict fields. A caller may persist the returned
//! annotation layer, but it must not grant the provider truth authority.

use std::fmt;
#[cfg(target_os = "macos")]
use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{json, Value};
use sysinfo::System;

/// Ollama should unload the optional language model shortly after a batch.
pub const KEEP_ALIVE: &str = "2m";
/// Increment when classification instructions or output semantics change.
pub const PROMPT_REVISION: &str = "m4.3-language-shadow-v2";

const GIB: u64 = 1024 * 1024 * 1024;
const MIB: u64 = 1024 * 1024;
const MAX_PROBE_RESPONSE_BYTES: u64 = 64 * 1024;
pub const DEFAULT_PAUSE_AVAILABLE_BYTES: u64 = 1024 * MIB;
pub const DEFAULT_RESUME_AVAILABLE_BYTES: u64 = 1536 * MIB;

const SYSTEM_PROMPT: &str = r#"Classify the language in one assistant message.
Return only the JSON object required by the response schema. Do not infer whether
work is true, supported, contradicted, fulfilled, or abandoned. You are not given
evidence and must not ask for it.

Categories:
- completed_claim: asserts a concrete action or machine-checkable outcome is complete
- proposal: affirmative durable intent to act later
- finding: discovery, diagnosis, measurement, or observation
- micro_intent: small immediate intention fulfilled in the current turn
- guidance: advice or an instruction, not a commitment by the speaker
- procedure: steps describing how to perform work, not a completion claim
- question: an interrogative or request for information
- narration: conversational glue or description of the speaker's current action
- uncertain: none of the categories can be assigned confidently

Also identify an optional speech act: proposal, decision, refusal, or deferral.
Use none when no listed speech act is explicit. A decision does not mean work
was completed. A refusal or deferral does not mean a thread was abandoned.

Provide a one-to-four-word plain-text navigation label when the category is not
uncertain. The label must only paraphrase the target sentence. It must not contain
a path, command, secret, Markdown, receipt claim, or truth verdict. Use null when
no safe label can be produced. When category is uncertain, speech_act must be
none and label must be null."#;

/// The closed M4 language-classification vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LanguageCategory {
    CompletedClaim,
    Proposal,
    Finding,
    MicroIntent,
    Guidance,
    Procedure,
    Question,
    Narration,
    Uncertain,
}

impl LanguageCategory {
    pub const ALL: [Self; 9] = [
        Self::CompletedClaim,
        Self::Proposal,
        Self::Finding,
        Self::MicroIntent,
        Self::Guidance,
        Self::Procedure,
        Self::Question,
        Self::Narration,
        Self::Uncertain,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::CompletedClaim => "completed_claim",
            Self::Proposal => "proposal",
            Self::Finding => "finding",
            Self::MicroIntent => "micro_intent",
            Self::Guidance => "guidance",
            Self::Procedure => "procedure",
            Self::Question => "question",
            Self::Narration => "narration",
            Self::Uncertain => "uncertain",
        }
    }
}

/// Optional speech-act navigation metadata. It is model-proposed shadow data,
/// never a lifecycle/evidence verdict and never a source-identity operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeechAct {
    None,
    Proposal,
    Decision,
    Refusal,
    Deferral,
}

impl SpeechAct {
    pub const ALL: [Self; 5] = [
        Self::None,
        Self::Proposal,
        Self::Decision,
        Self::Refusal,
        Self::Deferral,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Proposal => "proposal",
            Self::Decision => "decision",
            Self::Refusal => "refusal",
            Self::Deferral => "deferral",
        }
    }
}

/// A bounded model-proposed navigation label. Construction is private to the
/// validator so an accepted label cannot carry paths, commands, Markdown, or
/// common credential shapes into a later surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ShadowLabel(String);

impl ShadowLabel {
    pub fn new(value: impl Into<String>) -> Result<Self, LanguageError> {
        let value = value.into();
        validate_shadow_label(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for ShadowLabel {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

fn validate_shadow_label(value: &str) -> Result<(), LanguageError> {
    if value.trim() != value || value.is_empty() {
        return Err(LanguageError::InvalidOutput(
            "shadow label must be non-empty with no surrounding whitespace".to_string(),
        ));
    }
    let words = value.split_whitespace().count();
    if !(1..=4).contains(&words) || value.chars().count() > 64 {
        return Err(LanguageError::InvalidOutput(
            "shadow label must contain 1 to 4 words and at most 64 characters".to_string(),
        ));
    }
    let plain_words = value.split_whitespace().all(|word| {
        let characters: Vec<char> = word.chars().collect();
        characters.first().is_some_and(|c| c.is_alphanumeric())
            && characters.last().is_some_and(|c| c.is_alphanumeric())
            && characters.iter().enumerate().all(|(index, character)| {
                character.is_alphanumeric()
                    || (matches!(character, '-' | '\'' | '’')
                        && index > 0
                        && index + 1 < characters.len())
            })
    });
    if value.chars().any(char::is_control) || !plain_words {
        return Err(LanguageError::InvalidOutput(
            "shadow label contains path, command, Markdown, or control syntax".to_string(),
        ));
    }
    // A validity gate, not a redactor: a bare prefix is already
    // disqualifying here, even though it is not a mask-worthy shape (A209).
    let (_, _, masked) = crate::redact::mask(value);
    if masked > 0 || crate::redact::resembles_known_credential(value) {
        return Err(LanguageError::InvalidOutput(
            "shadow label contains a credential-shaped token".to_string(),
        ));
    }
    const TRUTH_AUTHORITY_TERMS: &[&str] = &[
        "abandoned",
        "complete",
        "completed",
        "contradicted",
        "created",
        "deployed",
        "deleted",
        "done",
        "dropped",
        "entailed",
        "false",
        "fixed",
        "finished",
        "fulfilled",
        "green",
        "implemented",
        "launched",
        "merged",
        "passed",
        "passing",
        "proven",
        "published",
        "ready",
        "recorded",
        "refactored",
        "released",
        "removed",
        "refuted",
        "resolved",
        "shipped",
        "succeeded",
        "success",
        "successful",
        "supported",
        "tested",
        "true",
        "unsupported",
        "unverified",
        "validated",
        "verified",
        "working",
        "works",
    ];
    const COMMAND_TERMS: &[&str] = &[
        "bash", "bun", "cargo", "curl", "deno", "docker", "dotnet", "git", "go", "gradle", "make",
        "mvn", "node", "npm", "npx", "pnpm", "python", "python3", "rm", "rustc", "sh", "sudo",
        "swift", "wget", "yarn", "zsh",
    ];
    let words = value
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    let has_truth_authority_term = words.iter().any(|word| {
        TRUTH_AUTHORITY_TERMS
            .iter()
            .any(|term| word.eq_ignore_ascii_case(term))
    });
    if has_truth_authority_term {
        return Err(LanguageError::InvalidOutput(
            "shadow label contains a truth-authority term".to_string(),
        ));
    }
    if words.iter().any(|word| {
        COMMAND_TERMS
            .iter()
            .any(|term| word.eq_ignore_ascii_case(term))
    }) {
        return Err(LanguageError::InvalidOutput(
            "shadow label contains a command term".to_string(),
        ));
    }
    Ok(())
}

/// A finite probability. Invalid model confidence is a parse failure, never
/// clamped into looking valid.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Confidence(f32);

impl Confidence {
    pub fn new(value: f32) -> Result<Self, LanguageError> {
        if value.is_finite() && (0.0..=1.0).contains(&value) {
            Ok(Self(value))
        } else {
            Err(LanguageError::InvalidOutput(
                "confidence must be a finite number in [0,1]".to_string(),
            ))
        }
    }

    pub fn get(self) -> f32 {
        self.0
    }
}

impl<'de> Deserialize<'de> for Confidence {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = f32::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// Strict model-authored JSON. Unknown keys fail deserialization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LanguageOutput {
    pub category: LanguageCategory,
    pub confidence: Confidence,
    pub speech_act: SpeechAct,
    pub label: Option<ShadowLabel>,
}

impl LanguageOutput {
    pub fn new(category: LanguageCategory, confidence: f32) -> Result<Self, LanguageError> {
        Ok(Self {
            category,
            confidence: Confidence::new(confidence)?,
            speech_act: SpeechAct::None,
            label: None,
        })
    }

    pub fn with_shadow(
        category: LanguageCategory,
        confidence: f32,
        speech_act: SpeechAct,
        label: Option<&str>,
    ) -> Result<Self, LanguageError> {
        let output = Self {
            category,
            confidence: Confidence::new(confidence)?,
            speech_act,
            label: label.map(ShadowLabel::new).transpose()?,
        };
        output.validate_relationships()?;
        Ok(output)
    }

    fn parse(text: &str) -> Result<Self, LanguageError> {
        let output: Self = serde_json::from_str(text)
            .map_err(|e| LanguageError::InvalidOutput(format!("invalid JSON output: {e}")))?;
        output.validate_relationships()?;
        Ok(output)
    }

    fn validate_relationships(&self) -> Result<(), LanguageError> {
        if self.category == LanguageCategory::Uncertain
            && (self.speech_act != SpeechAct::None || self.label.is_some())
        {
            return Err(LanguageError::InvalidOutput(
                "uncertain output must not carry a speech act or label".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UncertainReason {
    Provider(String),
    InvalidOutput(String),
}

/// Retry-once result. Provider failures never turn into a guessed category.
#[derive(Debug, Clone, PartialEq)]
pub enum ClassificationOutcome {
    Classified {
        output: LanguageOutput,
        attempts: u8,
    },
    Uncertain {
        attempts: u8,
        reason: UncertainReason,
    },
}

pub trait Annotator: Send + Sync {
    fn classify(&self, message: &str) -> ClassificationOutcome;
}

/// Default provider: language stays unclassified without any egress.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullAnnotator;

impl Annotator for NullAnnotator {
    fn classify(&self, _message: &str) -> ClassificationOutcome {
        ClassificationOutcome::Uncertain {
            attempts: 0,
            reason: UncertainReason::Provider("language provider disabled".to_string()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RamTier {
    Small,
    Mid,
    Full,
}

impl RamTier {
    pub fn for_total_bytes(total_bytes: u64) -> Self {
        if total_bytes >= 24 * GIB {
            Self::Full
        } else if total_bytes >= 16 * GIB {
            Self::Mid
        } else {
            Self::Small
        }
    }

    pub fn default_model(self) -> &'static str {
        match self {
            // The small tier stays within the brief's 0.6-1.5B range.
            Self::Small => "qwen3:0.6b",
            Self::Mid => "qwen3:4b",
            Self::Full => "qwen3:8b",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelSelection {
    pub tier: RamTier,
    pub model: String,
    pub overridden: bool,
}

/// Pure selection helper for config and tests.
pub fn select_model(
    total_bytes: u64,
    model_override: Option<&str>,
) -> Result<ModelSelection, LanguageError> {
    let tier = RamTier::for_total_bytes(total_bytes);
    let override_model = model_override.map(str::trim).filter(|m| !m.is_empty());
    if model_override.is_some() && override_model.is_none() {
        return Err(LanguageError::Config(
            "language model override cannot be empty".to_string(),
        ));
    }
    let (model, overridden) = match override_model {
        Some(model) => (model.to_string(), true),
        None => (tier.default_model().to_string(), false),
    };
    Ok(ModelSelection {
        tier,
        model,
        overridden,
    })
}

pub fn select_model_for_system(
    model_override: Option<&str>,
) -> Result<ModelSelection, LanguageError> {
    let mut system = System::new();
    system.refresh_memory();
    select_model(system.total_memory(), model_override)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryPressureState {
    Ready,
    Paused,
}

/// One memory-pressure observation. Production uses
/// [`SystemMemoryPressureProbe`]; deterministic tests may provide a local
/// scripted implementation without changing product configuration.
pub trait MemoryPressureProbe: Send + Sync {
    fn sample(
        &self,
        previous: MemoryPressureState,
        thresholds: MemoryPressureThresholds,
    ) -> Result<MemoryPressureState, LanguageError>;
}

/// The only pressure probe installed by the normal annotation path.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemMemoryPressureProbe;

impl MemoryPressureProbe for SystemMemoryPressureProbe {
    fn sample(
        &self,
        previous: MemoryPressureState,
        thresholds: MemoryPressureThresholds,
    ) -> Result<MemoryPressureState, LanguageError> {
        Ok(system_memory_pressure_state(previous, thresholds))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryPressureThresholds {
    pause_below_bytes: u64,
    resume_at_bytes: u64,
}

impl MemoryPressureThresholds {
    pub fn new(pause_below_bytes: u64, resume_at_bytes: u64) -> Result<Self, LanguageError> {
        if pause_below_bytes >= resume_at_bytes {
            return Err(LanguageError::Config(
                "memory resume threshold must exceed pause threshold".to_string(),
            ));
        }
        Ok(Self {
            pause_below_bytes,
            resume_at_bytes,
        })
    }

    pub fn pause_below_bytes(self) -> u64 {
        self.pause_below_bytes
    }

    pub fn resume_at_bytes(self) -> u64 {
        self.resume_at_bytes
    }
}

impl Default for MemoryPressureThresholds {
    fn default() -> Self {
        Self {
            pause_below_bytes: DEFAULT_PAUSE_AVAILABLE_BYTES,
            resume_at_bytes: DEFAULT_RESUME_AVAILABLE_BYTES,
        }
    }
}

/// Hysteretic memory-pressure transition. It prevents a batch from rapidly
/// alternating between pause and resume around a single threshold.
pub fn memory_pressure_state(
    previous: MemoryPressureState,
    available_bytes: u64,
    thresholds: MemoryPressureThresholds,
) -> MemoryPressureState {
    match previous {
        MemoryPressureState::Ready if available_bytes < thresholds.pause_below_bytes => {
            MemoryPressureState::Paused
        }
        MemoryPressureState::Paused if available_bytes >= thresholds.resume_at_bytes => {
            MemoryPressureState::Ready
        }
        state => state,
    }
}

pub fn system_memory_pressure_state(
    previous: MemoryPressureState,
    thresholds: MemoryPressureThresholds,
) -> MemoryPressureState {
    // sysinfo 0.37 subtracts every compressed page from reclaimable memory on
    // macOS. A healthy machine with inactive RAM can therefore report zero
    // "available" bytes forever. Apple's memorystatus level is the actual
    // pressure signal: 1 = normal, 2 = warning, 4 = critical. Fall back to the
    // byte thresholds if that read is unavailable.
    #[cfg(target_os = "macos")]
    if let Some(level) = macos_memory_pressure_level() {
        return memory_pressure_level_state(previous, level);
    }

    let mut system = System::new();
    system.refresh_memory();
    memory_pressure_state(previous, system.available_memory(), thresholds)
}

#[cfg(target_os = "macos")]
fn macos_memory_pressure_level() -> Option<u8> {
    let output = Command::new("/usr/sbin/sysctl")
        .args(["-n", "kern.memorystatus_vm_pressure_level"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    std::str::from_utf8(&output.stdout)
        .ok()?
        .trim()
        .parse()
        .ok()
}

#[cfg(target_os = "macos")]
fn memory_pressure_level_state(previous: MemoryPressureState, level: u8) -> MemoryPressureState {
    match previous {
        MemoryPressureState::Ready if level >= 2 => MemoryPressureState::Paused,
        MemoryPressureState::Paused if level <= 1 => MemoryPressureState::Ready,
        state => state,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OllamaOptions {
    pub base_url: String,
    pub model_override: Option<String>,
    pub connect_timeout: Duration,
    pub request_timeout: Duration,
    pub keep_alive: String,
}

impl Default for OllamaOptions {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:11434/v1".to_string(),
            model_override: None,
            connect_timeout: Duration::from_secs(2),
            request_timeout: Duration::from_secs(90),
            keep_alive: KEEP_ALIVE.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceAvailability {
    Available {
        models: Vec<String>,
        selected_model_present: bool,
        selected_model_digest: Option<String>,
    },
    Unavailable(String),
}

pub struct OllamaAnnotator {
    agent: ureq::Agent,
    base_url: String,
    native_base_url: String,
    selection: ModelSelection,
    keep_alive: String,
}

impl fmt::Debug for OllamaAnnotator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OllamaAnnotator")
            .field("base_url", &self.base_url)
            .field("selection", &self.selection)
            .finish_non_exhaustive()
    }
}

impl OllamaAnnotator {
    pub fn new(options: OllamaOptions) -> Result<Self, LanguageError> {
        let selection = select_model_for_system(options.model_override.as_deref())?;
        Self::with_selection(options, selection)
    }

    /// Constructor with an injected RAM value for callers that already took a
    /// system census and for deterministic tests.
    pub fn with_total_memory(
        options: OllamaOptions,
        total_memory_bytes: u64,
    ) -> Result<Self, LanguageError> {
        let selection = select_model(total_memory_bytes, options.model_override.as_deref())?;
        Self::with_selection(options, selection)
    }

    fn with_selection(
        options: OllamaOptions,
        selection: ModelSelection,
    ) -> Result<Self, LanguageError> {
        let base_url = normalise_local_base(&options.base_url)?;
        let native_base_url = base_url
            .strip_suffix("/v1")
            .ok_or_else(|| LanguageError::Config("Ollama base URL must end in /v1".to_string()))?
            .to_string();
        let keep_alive = options.keep_alive.trim();
        if keep_alive.is_empty() {
            return Err(LanguageError::Config(
                "Ollama keep-alive cannot be empty".to_string(),
            ));
        }
        let config = ureq::Agent::config_builder()
            .timeout_connect(Some(options.connect_timeout))
            .timeout_per_call(Some(options.request_timeout))
            // Environment proxy variables must never route transcript text
            // away from the explicitly validated loopback endpoint.
            .proxy(None)
            // A loopback endpoint must not be able to redirect sentence text
            // to a remote host. Local-only is an end-to-end property, not just
            // validation of the first URL.
            .max_redirects(0)
            .build();
        Ok(Self {
            agent: ureq::Agent::new_with_config(config),
            base_url,
            native_base_url,
            selection,
            keep_alive: keep_alive.to_string(),
        })
    }

    pub fn model_selection(&self) -> &ModelSelection {
        &self.selection
    }

    /// Read-only OpenAI-compatible service/model probe. This never loads or
    /// downloads a model.
    pub fn probe(&self) -> ServiceAvailability {
        let url = format!("{}/api/tags", self.native_base_url);
        let response = self.agent.get(&url).call();
        let mut response = match response {
            Ok(response) => response,
            Err(e) => return ServiceAvailability::Unavailable(e.to_string()),
        };
        let parsed: TagsResponse = match response
            .body_mut()
            .with_config()
            .limit(MAX_PROBE_RESPONSE_BYTES)
            .read_json()
        {
            Ok(value) => value,
            Err(e) => return ServiceAvailability::Unavailable(e.to_string()),
        };
        let mut selected_model_digest = None;
        let mut models = Vec::with_capacity(parsed.models.len());
        for model in parsed.models {
            if model.name == self.selection.model {
                if !valid_model_digest(&model.digest) {
                    return ServiceAvailability::Unavailable(
                        "selected Ollama model has no valid immutable digest".to_string(),
                    );
                }
                if selected_model_digest.replace(model.digest).is_some() {
                    return ServiceAvailability::Unavailable(
                        "selected Ollama model tag is ambiguous".to_string(),
                    );
                }
            }
            models.push(model.name);
        }
        models.sort();
        models.dedup();
        ServiceAvailability::Available {
            selected_model_present: selected_model_digest.is_some(),
            selected_model_digest,
            models,
        }
    }

    fn call_once(&self, message: &str) -> Result<LanguageOutput, LanguageError> {
        let request = ChatRequest {
            model: &self.selection.model,
            messages: vec![
                ChatMessage {
                    role: "system",
                    content: SYSTEM_PROMPT,
                },
                ChatMessage {
                    role: "user",
                    content: message,
                },
            ],
            response_format: response_format(),
            stream: false,
            temperature: 0.0,
            seed: 0,
            keep_alive: &self.keep_alive,
        };
        let url = format!("{}/chat/completions", self.base_url);
        let mut response = self
            .agent
            .post(&url)
            .header("Authorization", "Bearer ollama")
            .send_json(&request)
            .map_err(|e| LanguageError::Provider(e.to_string()))?;
        let parsed: Result<ChatResponse, LanguageError> = response
            .body_mut()
            .read_json()
            .map_err(|e| LanguageError::Provider(e.to_string()));
        self.refresh_keep_alive()?;
        let response = parsed?;
        if response.choices.len() != 1 {
            return Err(LanguageError::InvalidOutput(format!(
                "expected exactly one choice, received {}",
                response.choices.len()
            )));
        }
        let content = response
            .choices
            .first()
            .map(|choice| choice.message.content.as_str())
            .ok_or_else(|| LanguageError::InvalidOutput("missing response content".to_string()))?;
        LanguageOutput::parse(content)
    }

    /// Ollama's OpenAI-compatible endpoint currently accepts but ignores the
    /// `keep_alive` extension. A prompt-free native lifecycle request makes
    /// the configured TTL effective without giving the model any additional
    /// language or truth task.
    fn refresh_keep_alive(&self) -> Result<(), LanguageError> {
        let url = format!("{}/api/generate", self.native_base_url);
        self.agent
            .post(&url)
            .send_json(&KeepAliveRequest {
                model: &self.selection.model,
                keep_alive: &self.keep_alive,
            })
            .map_err(|e| LanguageError::Provider(format!("setting Ollama keep_alive: {e}")))?;
        Ok(())
    }
}

impl Annotator for OllamaAnnotator {
    fn classify(&self, message: &str) -> ClassificationOutcome {
        retry_once(|| self.call_once(message))
    }
}

fn retry_once<F>(mut operation: F) -> ClassificationOutcome
where
    F: FnMut() -> Result<LanguageOutput, LanguageError>,
{
    let mut last_error = None;
    for attempt in 1..=2 {
        match operation() {
            Ok(output) => {
                return ClassificationOutcome::Classified {
                    output,
                    attempts: attempt,
                }
            }
            Err(error) => last_error = Some(error),
        }
    }
    let reason = match last_error {
        Some(LanguageError::InvalidOutput(message)) => UncertainReason::InvalidOutput(message),
        Some(error) => UncertainReason::Provider(error.to_string()),
        None => UncertainReason::Provider("provider returned no result".to_string()),
    };
    ClassificationOutcome::Uncertain {
        attempts: 2,
        reason,
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LanguageError {
    #[error("language provider configuration: {0}")]
    Config(String),
    #[error("language provider request failed: {0}")]
    Provider(String),
    #[error("language provider output rejected: {0}")]
    InvalidOutput(String),
    #[error("language memory-pressure probe failed: {0}")]
    MemoryPressure(String),
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    response_format: Value,
    stream: bool,
    temperature: f32,
    seed: u64,
    /// Ollama extension. Kept on every request even through its OpenAI facade.
    keep_alive: &'a str,
}

#[derive(Serialize)]
struct KeepAliveRequest<'a> {
    model: &'a str,
    keep_alive: &'a str,
}

#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatResponseMessage,
}

#[derive(Deserialize)]
struct ChatResponseMessage {
    content: String,
}

#[derive(Deserialize)]
struct TagsResponse {
    models: Vec<TagInfo>,
}

#[derive(Deserialize)]
struct TagInfo {
    name: String,
    digest: String,
}

fn valid_model_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

/// Stable classification identity used by append-only storage. The tag keeps
/// the operator-facing name; the digest prevents locally retagged weights from
/// inheriting a frozen annotation created by different model bytes.
pub fn immutable_model_identity(tag: &str, digest: &str) -> Result<String, LanguageError> {
    let tag = tag.trim();
    if tag.is_empty() || !valid_model_digest(digest) {
        return Err(LanguageError::Config(
            "language model identity requires a tag and lowercase 64-hex digest".to_string(),
        ));
    }
    Ok(format!("{tag}@sha256:{digest}"))
}

fn response_format() -> Value {
    let categories: Vec<&str> = LanguageCategory::ALL
        .iter()
        .map(|category| category.as_str())
        .collect();
    let speech_acts: Vec<&str> = SpeechAct::ALL.iter().map(|act| act.as_str()).collect();
    json!({
        "type": "json_schema",
        "json_schema": {
            "name": "thruline_language_annotation",
            "strict": true,
            "schema": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "category": { "type": "string", "enum": categories },
                    "confidence": { "type": "number", "minimum": 0, "maximum": 1 },
                    "speech_act": { "type": "string", "enum": speech_acts },
                    "label": {
                        "anyOf": [
                            { "type": "string", "minLength": 1, "maxLength": 64 },
                            { "type": "null" }
                        ]
                    }
                },
                "required": ["category", "confidence", "speech_act", "label"]
            }
        }
    })
}

/// Canonical material for annotation idempotency and provenance hashes.
/// It deliberately includes the complete instructions, schema, and exact
/// message so changing any model-visible input invalidates the prior key.
pub fn prompt_material(user_message: &str) -> String {
    json!({
        "revision": PROMPT_REVISION,
        "system_prompt": SYSTEM_PROMPT,
        "response_format": response_format(),
        "user_message": user_message,
    })
    .to_string()
}

fn normalise_local_base(input: &str) -> Result<String, LanguageError> {
    let base = input.trim().trim_end_matches('/');
    let authority = base
        .strip_prefix("http://")
        .ok_or_else(|| LanguageError::Config("Ollama base URL must use local HTTP".to_string()))?
        .split('/')
        .next()
        .ok_or_else(|| LanguageError::Config("Ollama base URL has no authority".to_string()))?;
    if !is_local_authority(authority) {
        return Err(LanguageError::Config(
            "Ollama base URL must resolve to localhost".to_string(),
        ));
    }
    if base.ends_with("/v1") {
        Ok(base.to_string())
    } else {
        Ok(format!("{base}/v1"))
    }
}

fn is_local_authority(authority: &str) -> bool {
    if let Some(bracketed) = authority.strip_prefix('[') {
        let Some((host, suffix)) = bracketed.split_once(']') else {
            return false;
        };
        return host == "::1" && valid_port_suffix(suffix);
    }

    let (host, suffix) = match authority.split_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (authority, None),
    };
    matches!(
        host.to_ascii_lowercase().as_str(),
        "127.0.0.1" | "localhost"
    ) && suffix.is_none_or(|port| !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()))
}

fn valid_port_suffix(suffix: &str) -> bool {
    suffix.is_empty()
        || suffix
            .strip_prefix(':')
            .is_some_and(|port| !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};

    fn read_request(mut stream: &TcpStream) -> (String, Value) {
        let mut bytes = Vec::new();
        let mut buffer = [0_u8; 2048];
        let (header_end, content_length) = loop {
            let read = stream.read(&mut buffer).expect("read request");
            assert!(read > 0, "request ended before its headers");
            bytes.extend_from_slice(&buffer[..read]);
            let Some(header_end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") else {
                continue;
            };
            let headers = std::str::from_utf8(&bytes[..header_end]).expect("UTF-8 headers");
            let content_length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().expect("content length"))
                })
                .expect("content-length header");
            break (header_end, content_length);
        };
        let body_start = header_end + 4;
        while bytes.len() < body_start + content_length {
            let read = stream.read(&mut buffer).expect("read request body");
            assert!(read > 0, "request ended before its body");
            bytes.extend_from_slice(&buffer[..read]);
        }
        let first_line = std::str::from_utf8(&bytes[..header_end])
            .expect("headers")
            .lines()
            .next()
            .expect("request line");
        let path = first_line
            .split_whitespace()
            .nth(1)
            .expect("request path")
            .to_string();
        let body = serde_json::from_slice(&bytes[body_start..body_start + content_length])
            .expect("JSON request body");
        (path, body)
    }

    fn write_response(mut stream: &TcpStream, body: &str) {
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .expect("write response");
    }

    fn read_request_head(mut stream: &TcpStream) -> String {
        let mut bytes = Vec::new();
        let mut buffer = [0_u8; 1024];
        while !bytes.windows(4).any(|window| window == b"\r\n\r\n") {
            let read = stream.read(&mut buffer).expect("read request head");
            assert!(read > 0, "request ended before its headers");
            bytes.extend_from_slice(&buffer[..read]);
        }
        String::from_utf8(bytes).expect("UTF-8 request head")
    }

    #[test]
    fn category_domain_is_closed_and_stable() {
        let names: Vec<&str> = LanguageCategory::ALL.iter().map(|c| c.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "completed_claim",
                "proposal",
                "finding",
                "micro_intent",
                "guidance",
                "procedure",
                "question",
                "narration",
                "uncertain",
            ]
        );
    }

    #[test]
    fn model_probe_binds_the_selected_tag_to_an_immutable_digest() {
        let digest = "a".repeat(64);
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock Ollama");
        let address = listener.local_addr().expect("mock address");
        let expected_digest = digest.clone();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept probe");
            let head = read_request_head(&stream);
            let body = serde_json::json!({
                "models": [
                    {"name":"fixture:1b","digest":expected_digest},
                    {"name":"other:1b","digest":"b".repeat(64)}
                ]
            })
            .to_string();
            write_response(&stream, &body);
            head
        });
        let provider = OllamaAnnotator::with_total_memory(
            OllamaOptions {
                base_url: format!("http://{address}/v1"),
                model_override: Some("fixture:1b".to_string()),
                ..OllamaOptions::default()
            },
            8 * GIB,
        )
        .expect("provider");
        assert_eq!(
            provider.probe(),
            ServiceAvailability::Available {
                models: vec!["fixture:1b".to_string(), "other:1b".to_string()],
                selected_model_present: true,
                selected_model_digest: Some(digest.clone()),
            }
        );
        assert!(server
            .join()
            .expect("probe server")
            .starts_with("GET /api/tags HTTP/1.1"));
        assert_eq!(
            immutable_model_identity("fixture:1b", &digest).expect("identity"),
            format!("fixture:1b@sha256:{digest}")
        );
    }

    #[test]
    fn invalid_or_ambiguous_model_digests_fail_the_probe_closed() {
        for models in [
            serde_json::json!([{"name":"fixture:1b","digest":"not-a-digest"}]),
            serde_json::json!([
                {"name":"fixture:1b","digest":"a".repeat(64)},
                {"name":"fixture:1b","digest":"b".repeat(64)}
            ]),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock Ollama");
            let address = listener.local_addr().expect("mock address");
            let server = std::thread::spawn(move || {
                let (stream, _) = listener.accept().expect("accept probe");
                let _ = read_request_head(&stream);
                write_response(&stream, &serde_json::json!({"models":models}).to_string());
            });
            let provider = OllamaAnnotator::with_total_memory(
                OllamaOptions {
                    base_url: format!("http://{address}/v1"),
                    model_override: Some("fixture:1b".to_string()),
                    ..OllamaOptions::default()
                },
                8 * GIB,
            )
            .expect("provider");
            assert!(matches!(
                provider.probe(),
                ServiceAvailability::Unavailable(_)
            ));
            server.join().expect("probe server");
        }
    }

    #[test]
    fn strict_output_rejects_unknown_fields_and_bad_confidence() {
        let extra = r#"{"category":"finding","confidence":0.8,"speech_act":"none","label":"Parser finding","verdict":"true"}"#;
        assert!(LanguageOutput::parse(extra).is_err());
        for bad in ["-0.1", "1.1"] {
            let text = format!(
                "{{\"category\":\"finding\",\"confidence\":{bad},\"speech_act\":\"none\",\"label\":null}}"
            );
            assert!(LanguageOutput::parse(&text).is_err());
        }
    }

    #[test]
    fn strict_output_accepts_a_valid_closed_category() {
        let output = LanguageOutput::parse(
            r#"{"category":"micro_intent","confidence":0.91,"speech_act":"none","label":"Check parser"}"#,
        )
        .expect("valid output");
        assert_eq!(output.category, LanguageCategory::MicroIntent);
        assert_eq!(output.confidence.get(), 0.91);
        assert_eq!(output.speech_act, SpeechAct::None);
        assert_eq!(output.label.expect("label").as_str(), "Check parser");
    }

    #[test]
    fn shadow_labels_and_uncertain_relationships_fail_closed() {
        for label in [
            "five separate words are not allowed",
            "src/lib.rs",
            "`cargo test`",
            "sk-secret",
            "AKIAIOSFODNN7EXAMPLE",
            "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.signature",
            "Verified build success",
            "Work is supported",
            "Claim was contradicted",
            "Completed source",
            "Build succeeded",
            "Build finished",
            "Feature released",
            "Tests green",
            "cargo test",
            "go test",
            "rm -rf",
            "# heading",
            "*emphasis*",
            "_emphasis_",
            "line\nfeed",
        ] {
            assert!(ShadowLabel::new(label).is_err(), "accepted {label:?}");
        }
        assert!(LanguageOutput::parse(
            r#"{"category":"uncertain","confidence":0.2,"speech_act":"decision","label":null}"#,
        )
        .is_err());
        assert!(LanguageOutput::parse(
            r#"{"category":"uncertain","confidence":0.2,"speech_act":"none","label":"Maybe work"}"#,
        )
        .is_err());
        assert!(LanguageOutput::parse(
            r#"{"category":"uncertain","confidence":0.2,"speech_act":"none","label":null}"#,
        )
        .is_ok());
    }

    #[test]
    fn ram_tiers_and_override_are_deterministic() {
        assert_eq!(RamTier::for_total_bytes(8 * GIB), RamTier::Small);
        assert_eq!(RamTier::for_total_bytes(16 * GIB), RamTier::Mid);
        assert_eq!(RamTier::for_total_bytes(24 * GIB), RamTier::Full);
        assert_eq!(
            select_model(24 * GIB, None).expect("auto").model,
            "qwen3:8b"
        );
        let overridden = select_model(8 * GIB, Some("llama3.2:1b")).expect("override");
        assert_eq!(overridden.tier, RamTier::Small);
        assert_eq!(overridden.model, "llama3.2:1b");
        assert!(overridden.overridden);
    }

    #[test]
    fn memory_pressure_uses_hysteresis() {
        let thresholds = MemoryPressureThresholds::default();
        assert_eq!(
            memory_pressure_state(
                MemoryPressureState::Ready,
                DEFAULT_PAUSE_AVAILABLE_BYTES - 1,
                thresholds,
            ),
            MemoryPressureState::Paused
        );
        assert_eq!(
            memory_pressure_state(
                MemoryPressureState::Paused,
                DEFAULT_PAUSE_AVAILABLE_BYTES + 1,
                thresholds,
            ),
            MemoryPressureState::Paused
        );
        assert_eq!(
            memory_pressure_state(
                MemoryPressureState::Paused,
                DEFAULT_RESUME_AVAILABLE_BYTES,
                thresholds,
            ),
            MemoryPressureState::Ready
        );
        let configured = MemoryPressureThresholds::new(10, 20).expect("valid thresholds");
        assert_eq!(
            memory_pressure_state(MemoryPressureState::Ready, 9, configured),
            MemoryPressureState::Paused
        );
        assert!(MemoryPressureThresholds::new(20, 20).is_err());
    }

    #[test]
    fn system_memory_pressure_probe_returns_one_legitimate_sample_without_waiting() {
        let state = SystemMemoryPressureProbe
            .sample(
                MemoryPressureState::Ready,
                MemoryPressureThresholds::default(),
            )
            .expect("system pressure sample");
        assert!(matches!(
            state,
            MemoryPressureState::Ready | MemoryPressureState::Paused
        ));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_memory_pressure_levels_fail_closed_without_treating_compression_as_pressure() {
        assert_eq!(
            memory_pressure_level_state(MemoryPressureState::Ready, 1),
            MemoryPressureState::Ready
        );
        assert_eq!(
            memory_pressure_level_state(MemoryPressureState::Ready, 2),
            MemoryPressureState::Paused
        );
        assert_eq!(
            memory_pressure_level_state(MemoryPressureState::Paused, 4),
            MemoryPressureState::Paused
        );
        assert_eq!(
            memory_pressure_level_state(MemoryPressureState::Paused, 1),
            MemoryPressureState::Ready
        );
    }

    #[test]
    fn provider_accepts_only_local_openai_bases() {
        assert_eq!(
            normalise_local_base("http://127.0.0.1:11434").expect("local"),
            "http://127.0.0.1:11434/v1"
        );
        assert_eq!(
            normalise_local_base("http://localhost:11434/v1/").expect("local"),
            "http://localhost:11434/v1"
        );
        assert_eq!(
            normalise_local_base("http://[::1]:11434/v1").expect("IPv6 loopback"),
            "http://[::1]:11434/v1"
        );
        assert!(normalise_local_base("https://example.com/v1").is_err());
        assert!(normalise_local_base("http://example.com/v1").is_err());
        assert!(normalise_local_base("http://127.0.0.1:invalid/v1").is_err());

        let provider = OllamaAnnotator::with_total_memory(OllamaOptions::default(), 8 * GIB)
            .expect("local provider");
        assert_eq!(
            provider.agent.config().max_redirects(),
            0,
            "a local server must not redirect model-visible text off machine"
        );
        assert!(
            provider.agent.config().proxy().is_none(),
            "environment proxies must not receive model-visible text"
        );
    }

    #[test]
    fn keep_alive_is_configurable_and_defaults_to_two_minutes() {
        let default_provider =
            OllamaAnnotator::with_total_memory(OllamaOptions::default(), 8 * GIB)
                .expect("valid defaults");
        assert_eq!(default_provider.keep_alive, "2m");

        let options = OllamaOptions {
            keep_alive: "30s".to_string(),
            ..OllamaOptions::default()
        };
        let configured = OllamaAnnotator::with_total_memory(options, 8 * GIB)
            .expect("valid configured keep-alive");
        assert_eq!(configured.keep_alive, "30s");

        let invalid = OllamaOptions {
            keep_alive: " ".to_string(),
            ..OllamaOptions::default()
        };
        assert!(OllamaAnnotator::with_total_memory(invalid, 8 * GIB).is_err());
    }

    #[test]
    fn openai_classification_is_followed_by_a_prompt_free_native_ttl_request() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock Ollama");
        let address = listener.local_addr().expect("mock address");
        let server = std::thread::spawn(move || {
            let chat_body = serde_json::json!({
                "choices": [{
                    "message": {
                        "content": "{\"category\":\"finding\",\"confidence\":0.91,\"speech_act\":\"none\",\"label\":\"Diagnosis summary\"}"
                    }
                }]
            })
            .to_string();
            let mut requests = Vec::new();
            for response in [chat_body.as_str(), "{}"] {
                let (stream, _) = listener.accept().expect("accept request");
                requests.push(read_request(&stream));
                write_response(&stream, response);
            }
            requests
        });

        let options = OllamaOptions {
            base_url: format!("http://{address}/v1"),
            model_override: Some("fixture:1b".to_string()),
            keep_alive: "2m".to_string(),
            ..OllamaOptions::default()
        };
        let provider = OllamaAnnotator::with_total_memory(options, 8 * GIB).expect("provider");
        let result = provider.classify("Target sentence: A diagnosis was recorded.");
        assert!(matches!(
            result,
            ClassificationOutcome::Classified {
                output: LanguageOutput {
                    category: LanguageCategory::Finding,
                    ..
                },
                attempts: 1,
            }
        ));

        let requests = server.join().expect("mock server");
        assert_eq!(requests[0].0, "/v1/chat/completions");
        assert_eq!(requests[1].0, "/api/generate");
        assert_eq!(requests[1].1["model"], "fixture:1b");
        assert_eq!(requests[1].1["keep_alive"], "2m");
        assert_eq!(requests[1].1.as_object().expect("object").len(), 2);
    }

    #[test]
    fn null_provider_is_uncertain_without_attempting_a_call() {
        assert_eq!(
            NullAnnotator.classify("I changed the parser."),
            ClassificationOutcome::Uncertain {
                attempts: 0,
                reason: UncertainReason::Provider("language provider disabled".to_string()),
            }
        );
    }

    #[test]
    fn provider_retries_once_then_returns_uncertain() {
        let mut attempts = 0;
        let outcome = retry_once(|| {
            attempts += 1;
            Err(LanguageError::Provider(format!("failure {attempts}")))
        });
        assert_eq!(attempts, 2);
        assert_eq!(
            outcome,
            ClassificationOutcome::Uncertain {
                attempts: 2,
                reason: UncertainReason::Provider(
                    "language provider request failed: failure 2".to_string(),
                ),
            }
        );
    }

    #[test]
    fn provider_accepts_a_retry_success() {
        let mut attempts = 0;
        let outcome = retry_once(|| {
            attempts += 1;
            if attempts == 1 {
                return Err(LanguageError::Provider("transient".to_string()));
            }
            LanguageOutput::parse(
                r#"{"category":"narration","confidence":0.75,"speech_act":"none","label":"Current narration"}"#,
            )
        });
        assert_eq!(attempts, 2);
        assert!(matches!(
            outcome,
            ClassificationOutcome::Classified { attempts: 2, .. }
        ));
    }

    #[test]
    fn request_schema_contains_no_truth_or_evidence_fields() {
        let schema = response_format().to_string();
        assert!(!schema.contains("verdict"));
        assert!(!schema.contains("evidence"));
        assert!(schema.contains("completed_claim"));
        assert!(schema.contains("micro_intent"));
        assert!(schema.contains("uncertain"));
    }

    #[test]
    fn request_types_expose_only_language_inputs() {
        let request = ChatRequest {
            model: "example:1b",
            messages: vec![ChatMessage {
                role: "user",
                content: "One sentence.",
            }],
            response_format: response_format(),
            stream: false,
            temperature: 0.0,
            seed: 0,
            keep_alive: KEEP_ALIVE,
        };
        let value = serde_json::to_value(request).expect("serializable request");
        let object = value.as_object().expect("request object");
        assert_eq!(object.len(), 7);
        assert!(!object.contains_key("evidence"));
        assert!(!object.contains_key("verdict"));
        let messages = object["messages"].as_array().expect("messages array");
        assert_eq!(messages.len(), 1);
        let message = messages[0].as_object().expect("message object");
        assert_eq!(message.len(), 2);
        assert!(message.contains_key("role"));
        assert!(message.contains_key("content"));
    }

    #[test]
    fn prompt_material_tracks_instructions_schema_and_exact_message() {
        let first = prompt_material("Line one.\nLine two.");
        let second = prompt_material("Line one.\nLine two!");
        assert_ne!(first, second);
        let value: Value = serde_json::from_str(&first).expect("canonical JSON material");
        assert_eq!(value["revision"], PROMPT_REVISION);
        assert_eq!(value["system_prompt"], SYSTEM_PROMPT);
        assert_eq!(value["response_format"], response_format());
        assert_eq!(value["user_message"], "Line one.\nLine two.");
    }
}
