//! M5.R1 Assured Capture: local, signed, append-only execution-state evidence.
//!
//! This module deliberately has no truth-verdict API. It can bind an existing
//! immutable tool-result event to a complete execution receipt and derive the
//! independent `current | stale | unknown` **applicability** dimension — the
//! third proof axis in NORTHSTAR-v2's state model, answering *does this receipt
//! still cover present project state?* Unknown or unsupported input always
//! abstains.
//!
//! Applicability is an axis of the **receipt**, not of the claim. It is the
//! input to the A87 derivation rule — a `verified` claim reads `stale` exactly
//! when its receipt's applicability has lapsed — and it is deliberately NOT the
//! same thing as `Currency` (`current | superseded | out_of_scope`), which asks
//! whether a claim is still the operative one. The type was named
//! `ProofCurrentness` until A87 gave the axis its doctrinal name; variants and
//! every serialised string are unchanged, so no stored value moved.

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand_core::OsRng;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::chain::{sha256_hex, to_hex};
use crate::error::{CoreError, Result};
use crate::ledger::Ledger;

pub const CAPSULE_SCHEMA_VERSION: u32 = 1;
pub const MAX_STABILIZATION_ATTEMPTS: usize = 3;
pub const MAX_TRACKED_FILES: usize = 50_000;
pub const MAX_UNTRACKED_FILES: usize = 10_000;
pub const MAX_HASH_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureStrength {
    Assured,
    BoundedObservation,
    HistoricalOnly,
    Unknown,
}

impl CaptureStrength {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Assured => "assured",
            Self::BoundedObservation => "bounded_observation",
            Self::HistoricalOnly => "historical_only",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapsulePhase {
    Pre,
    Post,
    Observation,
}

impl CapsulePhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pre => "pre",
            Self::Post => "post",
            Self::Observation => "observation",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProofApplicability {
    Current,
    Stale,
    Unknown,
}

impl ProofApplicability {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Stale => "stale",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Completeness {
    pub git_repository: bool,
    pub committed_head: bool,
    pub ordinary_or_linked_worktree: bool,
    pub index: bool,
    pub tracked_worktree: bool,
    pub untracked: bool,
    pub ignored_inputs_excluded: bool,
    pub no_nested_repository: bool,
    pub no_submodule: bool,
    pub no_sparse_checkout: bool,
    pub recognized_profile: bool,
    pub executable: bool,
    pub toolchain: bool,
    pub lock_config: bool,
    pub environment: bool,
    pub stabilized: bool,
    pub failures: Vec<String>,
}

impl Completeness {
    pub fn complete(&self) -> bool {
        self.git_repository
            && self.committed_head
            && self.ordinary_or_linked_worktree
            && self.index
            && self.tracked_worktree
            && self.untracked
            && self.ignored_inputs_excluded
            && self.no_nested_repository
            && self.no_submodule
            && self.no_sparse_checkout
            && self.recognized_profile
            && self.executable
            && self.toolchain
            && self.lock_config
            && self.environment
            && self.stabilized
            && self.failures.is_empty()
    }

    fn new() -> Self {
        Self {
            git_repository: false,
            committed_head: false,
            ordinary_or_linked_worktree: false,
            index: false,
            tracked_worktree: false,
            untracked: false,
            ignored_inputs_excluded: false,
            no_nested_repository: false,
            no_submodule: false,
            no_sparse_checkout: false,
            recognized_profile: false,
            executable: false,
            toolchain: false,
            lock_config: false,
            environment: false,
            stabilized: false,
            failures: Vec::new(),
        }
    }

    fn fail(&mut self, code: &str) {
        if !self.failures.iter().any(|stored| stored == code) {
            self.failures.push(code.to_string());
            self.failures.sort();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateFingerprint {
    pub repository_id: Option<String>,
    pub common_dir_id: Option<String>,
    pub worktree_id: Option<String>,
    pub platform_id: String,
    pub head_ref_digest: Option<String>,
    pub detached: bool,
    pub operation_state_digest: Option<String>,
    pub head_commit: Option<String>,
    pub head_tree: Option<String>,
    pub index_digest: Option<String>,
    pub staged_digest: Option<String>,
    pub tracked_worktree_digest: Option<String>,
    pub untracked_digest: Option<String>,
    pub toolchain_digest: Option<String>,
    pub lock_config_digest: Option<String>,
    pub environment_digest: Option<String>,
    pub profile_digest: Option<String>,
    pub tracked_files: u64,
    pub untracked_files: u64,
    pub hashed_bytes: u64,
    pub completeness: Completeness,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapsulePayload {
    pub schema_version: u32,
    pub attempt_id: String,
    pub phase: CapsulePhase,
    pub prior_capsule_id: Option<String>,
    pub owner_key: String,
    pub source: String,
    pub source_project_key: String,
    pub source_session_key: String,
    #[serde(default)]
    pub source_turn_id: Option<String>,
    pub source_tool_id: String,
    pub command_sha256: String,
    pub source_config_sha256: String,
    pub source_version: String,
    pub profile: String,
    pub strength: CaptureStrength,
    pub observed_at_unix_ms: u64,
    pub capture_duration_ns: u64,
    pub state: StateFingerprint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedCapsule {
    pub capsule_id: String,
    pub key_id: String,
    pub signature_hex: String,
    pub payload: CapsulePayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttemptIntent {
    pub schema_version: u32,
    pub attempt_id: String,
    pub owner_key: String,
    pub source: String,
    pub source_project_key: String,
    pub source_session_key: String,
    #[serde(default)]
    pub source_turn_id: Option<String>,
    pub source_tool_id: String,
    pub wrapper_nonce: String,
    pub command_sha256: String,
    pub source_config_sha256: String,
    pub source_version: String,
    pub command: String,
    pub cwd: String,
    pub created_at_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrapperReceipt {
    pub schema_version: u32,
    pub attempt_id: String,
    pub pre_capsule_id: Option<String>,
    pub post_capsule_id: Option<String>,
    pub started_monotonic_ns: u64,
    pub ended_monotonic_ns: u64,
    pub exit_code: Option<i32>,
    pub term_signal: Option<i32>,
    pub strength: CaptureStrength,
    pub failure_code: Option<String>,
    pub completed_at_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceDelivery {
    pub schema_version: u32,
    pub attempt_id: String,
    pub owner_key: String,
    pub source: String,
    pub source_project_key: String,
    pub source_session_key: String,
    #[serde(default)]
    pub source_turn_id: Option<String>,
    pub source_tool_id: String,
    pub wrapper_nonce: String,
    pub source_event_id: String,
    pub source_exit_code: Option<i32>,
    pub source_term_signal: Option<i32>,
    pub observed_at_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comparison {
    pub currentness: ProofApplicability,
    pub comparable_dimensions: Vec<String>,
    pub changed_dimensions: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureRequest {
    pub owner_key: String,
    pub source: String,
    pub source_project_key: String,
    pub source_session_key: String,
    #[serde(default)]
    pub source_turn_id: Option<String>,
    pub source_tool_id: String,
    pub wrapper_nonce: String,
    pub command: String,
    pub cwd: PathBuf,
    pub source_config_sha256: String,
    pub source_version: String,
    pub assurance_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedInvocation {
    pub invocation_id: String,
    pub key_id: String,
    pub signature_hex: String,
    pub request: CaptureRequest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookDelivery {
    pub schema_version: u32,
    pub attempt_id: String,
    pub owner_key: String,
    pub source: String,
    pub source_project_key: String,
    pub source_session_key: String,
    #[serde(default)]
    pub source_turn_id: Option<String>,
    pub source_tool_id: String,
    pub wrapper_nonce: String,
    pub observed_at_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionOutcome {
    pub attempt_id: String,
    pub receipt: WrapperReceipt,
    pub receipt_sha256: String,
    pub pre: Option<SignedCapsule>,
    pub post: Option<SignedCapsule>,
    pub exit_code: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationOutcome {
    pub attempt_id: String,
    pub capsule: SignedCapsule,
    pub private_relative_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservationPublication {
    pub refresh_id: String,
    pub observation_capsule_id: String,
    pub execution_count: u64,
    pub comparison_count: u64,
    pub current_count: u64,
    pub stale_count: u64,
    pub unknown_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceAssuranceMeasurement {
    pub source: String,
    pub source_version: String,
    pub config_sha256: String,
    pub matcher: String,
    pub pre_blocks: bool,
    pub rewrite_exact: bool,
    pub stable_tool_identity: bool,
    pub completion_pairing: bool,
    pub measured_pre_delay_ms: u64,
    pub child_started_after_pre: bool,
    pub wrapper_exit_code: i32,
    pub source_exit_code: i32,
}

#[derive(Debug, Clone)]
pub struct CaptureAcceptance<'a> {
    pub outcome: &'a ExecutionOutcome,
    pub delivery: &'a SourceDelivery,
    pub pre_private_relative_ref: String,
    pub post_private_relative_ref: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureCommitFailpoint {
    AfterAttempt,
    AfterPre,
    AfterPost,
    BeforeLink,
    BeforeCommit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedCapture {
    pub attempt_id: String,
    pub event_id: String,
    pub pre_capsule_id: String,
    pub post_capsule_id: String,
    pub strength: CaptureStrength,
}

impl SourceAssuranceMeasurement {
    pub fn enables_assured(&self) -> bool {
        self.source == "codex"
            && registration(&self.source, &self.matcher)
                .is_some_and(|r| r.status == RegistrationStatus::Registered)
            && self.pre_blocks
            && self.rewrite_exact
            && self.stable_tool_identity
            && self.completion_pairing
            && self.child_started_after_pre
            && self.wrapper_exit_code == self.source_exit_code
    }
}

pub struct Collector {
    root: PathBuf,
    owner_key: String,
    source: String,
    signing_key: SigningKey,
    key_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectorInspection {
    pub available: bool,
    pub key_id: Option<String>,
    pub permissions_valid: bool,
    pub reason: String,
}

impl Collector {
    /// Inspect the expected collector identity without creating directories,
    /// keys, receipts, or observations. Status and doctor use only this path.
    pub fn inspect(
        root: impl AsRef<Path>,
        owner_key: &str,
        source: &str,
    ) -> Result<CollectorInspection> {
        if owner_key.is_empty() || !matches!(source, "codex" | "claude_code") {
            return Err(CoreError::Config(
                "capture collector requires owner and recognized source".to_string(),
            ));
        }
        let scope = sha256_hex(format!("{owner_key}\0{source}").as_bytes());
        let path = root.as_ref().join("keys").join(format!("{scope}.ed25519"));
        if !path.exists() {
            return Ok(CollectorInspection {
                available: false,
                key_id: None,
                permissions_valid: false,
                reason: "collector key is absent".to_string(),
            });
        }
        let metadata = fs::symlink_metadata(&path).map_err(|error| CoreError::io(&path, error))?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Ok(CollectorInspection {
                available: false,
                key_id: None,
                permissions_valid: false,
                reason: "collector key is not a private regular file".to_string(),
            });
        }
        let bytes = read_bounded(&path, 32)?;
        if bytes.len() != 32 {
            return Ok(CollectorInspection {
                available: false,
                key_id: None,
                permissions_valid: false,
                reason: "collector key length is invalid".to_string(),
            });
        }
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&bytes);
        let signing = SigningKey::from_bytes(&seed);
        #[cfg(unix)]
        let permissions_valid = {
            use std::os::unix::fs::PermissionsExt;
            metadata.permissions().mode() & 0o077 == 0
        };
        #[cfg(not(unix))]
        let permissions_valid = true;
        Ok(CollectorInspection {
            available: true,
            key_id: Some(sha256_hex(signing.verifying_key().as_bytes())),
            permissions_valid,
            reason: if permissions_valid {
                "collector key is readable with private permissions".to_string()
            } else {
                "collector key permissions are not private".to_string()
            },
        })
    }

    pub fn open(root: impl AsRef<Path>, owner_key: &str, source: &str) -> Result<Self> {
        if owner_key.is_empty() || !matches!(source, "codex" | "claude_code") {
            return Err(CoreError::Config(
                "capture collector requires owner and recognized source".to_string(),
            ));
        }
        let root = root.as_ref().to_path_buf();
        secure_dir(&root)?;
        for child in [
            "keys",
            "invocations",
            "tool-index",
            "intents",
            "sealed",
            "deliveries",
            "orphaned",
            "rejected",
        ] {
            secure_dir(&root.join(child))?;
        }
        let scope = sha256_hex(format!("{owner_key}\0{source}").as_bytes());
        let key_path = root.join("keys").join(format!("{scope}.ed25519"));
        let signing_key = load_or_create_key(&key_path)?;
        let key_id = sha256_hex(signing_key.verifying_key().as_bytes());
        Ok(Self {
            root,
            owner_key: owner_key.to_string(),
            source: source.to_string(),
            signing_key,
            key_id,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    pub fn owner_key(&self) -> &str {
        &self.owner_key
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn public_key_hex(&self) -> String {
        to_hex(self.signing_key.verifying_key().as_bytes())
    }

    pub(crate) fn sign_record<T: Serialize>(&self, record: &T) -> Result<(String, String)> {
        let bytes = canonical_json(record)?;
        Ok((
            sha256_hex(&bytes),
            to_hex(&self.signing_key.sign(&bytes).to_bytes()),
        ))
    }

    pub fn write_intent(&self, intent: &AttemptIntent) -> Result<PathBuf> {
        if intent.owner_key != self.owner_key || intent.source != self.source {
            return Err(CoreError::Integrity(
                "capture intent crosses collector scope".to_string(),
            ));
        }
        validate_opaque_project_key(&intent.source_project_key)?;
        let path = self
            .root
            .join("intents")
            .join(format!("{}.intent.json", intent.attempt_id));
        write_exclusive_idempotent(&path, &canonical_json(intent)?)?;
        Ok(path)
    }

    pub fn write_invocation(&self, request: CaptureRequest) -> Result<SignedInvocation> {
        if request.owner_key != self.owner_key || request.source != self.source {
            return Err(CoreError::Integrity(
                "invocation crosses collector scope".to_string(),
            ));
        }
        validate_opaque_project_key(&request.source_project_key)?;
        let request_bytes = canonical_json(&request)?;
        let invocation_id = format!("inv_{}", sha256_hex(&request_bytes));
        let binding = [
            b"thruline-invocation-v1".as_slice(),
            invocation_id.as_bytes(),
            &request_bytes,
        ]
        .into_iter()
        .fold(Vec::new(), |mut output, field| {
            output.extend_from_slice(&(field.len() as u64).to_le_bytes());
            output.extend_from_slice(field);
            output
        });
        let invocation = SignedInvocation {
            invocation_id: invocation_id.clone(),
            key_id: self.key_id.clone(),
            signature_hex: to_hex(&self.signing_key.sign(&binding).to_bytes()),
            request,
        };
        let path = self
            .root
            .join("invocations")
            .join(format!("{invocation_id}.json"));
        write_exclusive_idempotent(&path, &canonical_json(&invocation)?)?;
        let index_key = hash_fields(&[
            &invocation.request.source_session_key,
            &invocation.request.source_tool_id,
        ]);
        let index_path = self
            .root
            .join("tool-index")
            .join(format!("{index_key}.json"));
        let index = serde_json::json!({
            "invocation_id": invocation_id,
            "attempt_id": attempt_id(
                &invocation.request,
                &sha256_hex(invocation.request.command.as_bytes())
            ),
            "wrapper_nonce": invocation.request.wrapper_nonce,
        });
        write_exclusive_idempotent(&index_path, &canonical_value(&index)?)?;
        Ok(invocation)
    }

    pub fn load_invocation(&self, invocation_id: &str) -> Result<SignedInvocation> {
        if !invocation_id.starts_with("inv_")
            || !invocation_id
                .bytes()
                .all(|byte| byte == b'_' || byte.is_ascii_alphanumeric())
        {
            return Err(CoreError::Integrity(
                "invalid invocation identity".to_string(),
            ));
        }
        let path = self
            .root
            .join("invocations")
            .join(format!("{invocation_id}.json"));
        let invocation: SignedInvocation =
            serde_json::from_slice(&read_bounded(&path, 64 * 1024)?)?;
        if invocation.invocation_id != invocation_id || invocation.key_id != self.key_id {
            return Err(CoreError::Integrity(
                "invocation identity or collector key mismatch".to_string(),
            ));
        }
        let request_bytes = canonical_json(&invocation.request)?;
        if format!("inv_{}", sha256_hex(&request_bytes)) != invocation.invocation_id {
            return Err(CoreError::Integrity(
                "invocation canonical digest mismatch".to_string(),
            ));
        }
        let binding = [
            b"thruline-invocation-v1".as_slice(),
            invocation.invocation_id.as_bytes(),
            &request_bytes,
        ]
        .into_iter()
        .fold(Vec::new(), |mut output, field| {
            output.extend_from_slice(&(field.len() as u64).to_le_bytes());
            output.extend_from_slice(field);
            output
        });
        let signature = Signature::from_bytes(&decode_hex_fixed::<64>(&invocation.signature_hex)?);
        self.signing_key
            .verifying_key()
            .verify(&binding, &signature)
            .map_err(|_| CoreError::Integrity("invocation signature failed".to_string()))?;
        Ok(invocation)
    }

    pub fn record_hook_delivery(
        &self,
        source_session_key: &str,
        source_turn_id: Option<&str>,
        source_tool_id: &str,
    ) -> Result<HookDelivery> {
        let index_key = hash_fields(&[source_session_key, source_tool_id]);
        let index_path = self
            .root
            .join("tool-index")
            .join(format!("{index_key}.json"));
        let index: Value = serde_json::from_slice(&read_bounded(&index_path, 16 * 1024)?)?;
        let invocation_id = index
            .get("invocation_id")
            .and_then(Value::as_str)
            .ok_or_else(|| CoreError::Integrity("tool index lacks invocation id".to_string()))?;
        let invocation = self.load_invocation(invocation_id)?;
        if invocation.request.source_session_key != source_session_key
            || invocation.request.source_turn_id.as_deref() != source_turn_id
            || invocation.request.source_tool_id != source_tool_id
        {
            return Err(CoreError::Integrity(
                "post-hook tool identity does not match PRE invocation".to_string(),
            ));
        }
        let attempt = index
            .get("attempt_id")
            .and_then(Value::as_str)
            .ok_or_else(|| CoreError::Integrity("tool index lacks attempt id".to_string()))?;
        let delivery = HookDelivery {
            schema_version: CAPSULE_SCHEMA_VERSION,
            attempt_id: attempt.to_string(),
            owner_key: self.owner_key.clone(),
            source: self.source.clone(),
            source_project_key: invocation.request.source_project_key,
            source_session_key: source_session_key.to_string(),
            source_turn_id: source_turn_id.map(str::to_string),
            source_tool_id: source_tool_id.to_string(),
            wrapper_nonce: invocation.request.wrapper_nonce,
            observed_at_unix_ms: unix_millis()?,
        };
        let path = self
            .root
            .join("deliveries")
            .join(format!("{}.hook.json", delivery.attempt_id));
        write_exclusive_idempotent(&path, &canonical_json(&delivery)?)?;
        Ok(delivery)
    }

    pub fn load_hook_delivery(&self, attempt_id: &str) -> Result<HookDelivery> {
        let path = self
            .root
            .join("deliveries")
            .join(format!("{attempt_id}.hook.json"));
        let delivery: HookDelivery = serde_json::from_slice(&read_bounded(&path, 16 * 1024)?)?;
        if delivery.attempt_id != attempt_id
            || delivery.owner_key != self.owner_key
            || delivery.source != self.source
        {
            return Err(CoreError::Integrity(
                "hook delivery crosses collector scope".to_string(),
            ));
        }
        Ok(delivery)
    }

    pub fn load_execution(&self, attempt_id: &str) -> Result<ExecutionOutcome> {
        let dir = self.root.join("sealed").join(attempt_id);
        let mut pre = None;
        let mut post = None;
        let mut receipt: Option<WrapperReceipt> = None;
        let mut receipt_sha256 = None;
        for entry in fs::read_dir(&dir).map_err(|e| CoreError::io(&dir, e))? {
            let entry = entry.map_err(|e| CoreError::io(&dir, e))?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("pre.") {
                pre = Some(serde_json::from_slice(&read_bounded(
                    &entry.path(),
                    64 * 1024,
                )?)?);
            } else if name.starts_with("post.") {
                post = Some(serde_json::from_slice(&read_bounded(
                    &entry.path(),
                    64 * 1024,
                )?)?);
            } else if name.starts_with("receipt.") {
                let bytes = read_bounded(&entry.path(), 64 * 1024)?;
                receipt_sha256 = Some(sha256_hex(&bytes));
                receipt = Some(serde_json::from_slice(&bytes)?);
            }
        }
        let receipt = receipt.ok_or_else(|| {
            CoreError::Integrity("capture attempt has no complete wrapper receipt".to_string())
        })?;
        if receipt.attempt_id != attempt_id {
            return Err(CoreError::Integrity(
                "wrapper receipt attempt identity mismatch".to_string(),
            ));
        }
        let receipt_sha256 = receipt_sha256
            .ok_or_else(|| CoreError::Integrity("wrapper receipt digest is absent".to_string()))?;
        let exit_code = receipt
            .exit_code
            .unwrap_or_else(|| 128 + receipt.term_signal.unwrap_or(1));
        Ok(ExecutionOutcome {
            attempt_id: attempt_id.to_string(),
            receipt,
            receipt_sha256,
            pre,
            post,
            exit_code,
        })
    }

    pub fn verify_accepted_capsules(&self, ledger: &Ledger) -> Result<usize> {
        let mut statement = ledger.conn().prepare(
            "SELECT s.capsule_id,s.attempt_id,s.phase,s.signature_hex,s.key_id,
                    s.private_relative_ref,s.complete,k.public_key_hex
             FROM state_capsules s
             JOIN capture_attempts a ON a.attempt_id=s.attempt_id
             JOIN collector_keys k ON k.key_id=s.key_id
             WHERE a.owner_key=?1 AND a.source=?2
             ORDER BY s.accepted_seq",
        )?;
        let rows = statement.query_map(params![self.owner_key, self.source], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, i64>(6)? != 0,
                row.get::<_, String>(7)?,
            ))
        })?;
        let mut verified = 0usize;
        for row in rows {
            let (id, attempt, phase, signature, key_id, relative, complete, public) = row?;
            validate_private_ref(&relative, &attempt)?;
            let path = self.root.join(&relative);
            let metadata = fs::symlink_metadata(&path).map_err(|e| CoreError::io(&path, e))?;
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err(CoreError::Integrity(format!(
                    "accepted capsule {id} has no real private payload"
                )));
            }
            let capsule: SignedCapsule = serde_json::from_slice(&read_bounded(&path, 64 * 1024)?)?;
            if capsule.capsule_id != id
                || capsule.payload.attempt_id != attempt
                || capsule.payload.phase.as_str() != phase
                || capsule.signature_hex != signature
                || capsule.key_id != key_id
                || capsule.payload.state.completeness.complete() != complete
                || key_id != sha256_hex(&decode_hex_fixed::<32>(&public)?)
            {
                return Err(CoreError::Integrity(format!(
                    "accepted capsule {id} disagrees with its append-only index"
                )));
            }
            Self::verify_capsule_with_public_key(&capsule, &public)?;
            verified += 1;
        }
        Ok(verified)
    }

    pub fn seal_capsule(&self, payload: CapsulePayload) -> Result<(SignedCapsule, PathBuf)> {
        if payload.owner_key != self.owner_key || payload.source != self.source {
            return Err(CoreError::Integrity(
                "capture capsule crosses collector scope".to_string(),
            ));
        }
        let payload_bytes = canonical_json(&payload)?;
        let capsule_id = sha256_hex(&payload_bytes);
        let binding = signature_binding(
            &capsule_id,
            &payload.attempt_id,
            payload.phase,
            payload.prior_capsule_id.as_deref(),
            payload.state.completeness.complete(),
        );
        let signature = self.signing_key.sign(&binding);
        let signed = SignedCapsule {
            capsule_id: capsule_id.clone(),
            key_id: self.key_id.clone(),
            signature_hex: to_hex(&signature.to_bytes()),
            payload,
        };
        let dir = self.root.join("sealed").join(&signed.payload.attempt_id);
        secure_dir(&dir)?;
        let path = dir.join(format!(
            "{}.{}.json",
            signed.payload.phase.as_str(),
            capsule_id
        ));
        write_exclusive_idempotent(&path, &canonical_json(&signed)?)?;
        Ok((signed, path))
    }

    pub fn write_receipt(&self, receipt: &WrapperReceipt) -> Result<(String, PathBuf)> {
        let bytes = canonical_json(receipt)?;
        let digest = sha256_hex(&bytes);
        let dir = self.root.join("sealed").join(&receipt.attempt_id);
        secure_dir(&dir)?;
        let path = dir.join(format!("receipt.{digest}.json"));
        write_exclusive_idempotent(&path, &bytes)?;
        Ok((digest, path))
    }

    pub fn write_delivery(&self, delivery: &SourceDelivery) -> Result<(String, PathBuf)> {
        if delivery.owner_key != self.owner_key || delivery.source != self.source {
            return Err(CoreError::Integrity(
                "source delivery crosses collector scope".to_string(),
            ));
        }
        let bytes = canonical_json(delivery)?;
        let digest = sha256_hex(&bytes);
        let path = self
            .root
            .join("deliveries")
            .join(format!("{}.{}.json", delivery.attempt_id, digest));
        write_exclusive_idempotent(&path, &bytes)?;
        Ok((digest, path))
    }

    pub fn verify_capsule(&self, capsule: &SignedCapsule) -> Result<()> {
        if capsule.key_id != self.key_id {
            return Err(CoreError::Integrity(
                "capsule names a different collector key".to_string(),
            ));
        }
        let canonical = canonical_json(&capsule.payload)?;
        let expected = sha256_hex(&canonical);
        if expected != capsule.capsule_id {
            return Err(CoreError::Integrity(
                "capsule canonical digest mismatch".to_string(),
            ));
        }
        let signature_bytes = decode_hex_fixed::<64>(&capsule.signature_hex)?;
        let signature = Signature::from_bytes(&signature_bytes);
        let binding = signature_binding(
            &capsule.capsule_id,
            &capsule.payload.attempt_id,
            capsule.payload.phase,
            capsule.payload.prior_capsule_id.as_deref(),
            capsule.payload.state.completeness.complete(),
        );
        self.signing_key
            .verifying_key()
            .verify(&binding, &signature)
            .map_err(|_| CoreError::Integrity("capsule signature verification failed".to_string()))
    }

    pub fn verify_capsule_with_public_key(
        capsule: &SignedCapsule,
        public_key_hex: &str,
    ) -> Result<()> {
        let public = decode_hex_fixed::<32>(public_key_hex)?;
        let verifying = VerifyingKey::from_bytes(&public)
            .map_err(|_| CoreError::Integrity("invalid collector public key".to_string()))?;
        let canonical = canonical_json(&capsule.payload)?;
        if sha256_hex(&canonical) != capsule.capsule_id {
            return Err(CoreError::Integrity(
                "capsule canonical digest mismatch".to_string(),
            ));
        }
        let signature_bytes = decode_hex_fixed::<64>(&capsule.signature_hex)?;
        let signature = Signature::from_bytes(&signature_bytes);
        let binding = signature_binding(
            &capsule.capsule_id,
            &capsule.payload.attempt_id,
            capsule.payload.phase,
            capsule.payload.prior_capsule_id.as_deref(),
            capsule.payload.state.completeness.complete(),
        );
        verifying
            .verify(&binding, &signature)
            .map_err(|_| CoreError::Integrity("capsule signature verification failed".to_string()))
    }

    fn verify_private_capsule_ref(&self, relative: &str, expected: &SignedCapsule) -> Result<()> {
        validate_private_ref(relative, &expected.payload.attempt_id)?;
        let path = self.root.join(relative);
        let metadata = fs::symlink_metadata(&path).map_err(|e| CoreError::io(&path, e))?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(CoreError::Integrity(
                "private capsule receipt is absent or symlinked".to_string(),
            ));
        }
        let stored: SignedCapsule = serde_json::from_slice(&read_bounded(&path, 64 * 1024)?)?;
        if &stored != expected {
            return Err(CoreError::Integrity(
                "private capsule bytes disagree with accepted capsule".to_string(),
            ));
        }
        self.verify_capsule(&stored)
    }

    /// Close unfinished durable intents without inventing a phase or result.
    /// Each orphan marker is immutable and idempotent; the original intent is
    /// retained as private diagnostic evidence.
    pub fn recover_orphans(&self) -> Result<usize> {
        let orphan_dir = self.root.join("orphaned");
        let mut already_recovered = BTreeSet::new();
        for entry in fs::read_dir(&orphan_dir).map_err(|e| CoreError::io(&orphan_dir, e))? {
            let entry = entry.map_err(|e| CoreError::io(&orphan_dir, e))?;
            if !entry
                .file_type()
                .map_err(|e| CoreError::io(entry.path(), e))?
                .is_file()
            {
                continue;
            }
            let value: Value = serde_json::from_slice(&read_bounded(&entry.path(), 512 * 1024)?)?;
            if let Some(attempt) = value.get("attempt_id").and_then(Value::as_str) {
                already_recovered.insert(attempt.to_string());
            }
            if let Some(attempts) = value.get("attempt_ids").and_then(Value::as_array) {
                for attempt in attempts.iter().filter_map(Value::as_str) {
                    already_recovered.insert(attempt.to_string());
                }
            }
        }
        let mut intents = Vec::new();
        for entry in fs::read_dir(self.root.join("intents"))
            .map_err(|e| CoreError::io(self.root.join("intents"), e))?
        {
            let entry = entry.map_err(|e| CoreError::io(self.root.join("intents"), e))?;
            if entry
                .file_type()
                .map_err(|e| CoreError::io(entry.path(), e))?
                .is_file()
            {
                intents.push(entry.path());
            }
        }
        intents.sort();
        let mut recovered = Vec::new();
        for path in intents {
            let bytes = read_bounded(&path, 64 * 1024)?;
            let intent: AttemptIntent = serde_json::from_slice(&bytes)?;
            let sealed = self.root.join("sealed").join(&intent.attempt_id);
            let complete = if sealed.is_dir() {
                fs::read_dir(&sealed)
                    .map_err(|e| CoreError::io(&sealed, e))?
                    .filter_map(std::result::Result::ok)
                    .any(|entry| entry.file_name().to_string_lossy().starts_with("receipt."))
            } else {
                false
            };
            if complete {
                continue;
            }
            if !already_recovered.contains(&intent.attempt_id) {
                recovered.push(intent.attempt_id);
            }
        }
        recovered.sort();
        if recovered.is_empty() {
            return Ok(0);
        }
        // One append-only batch receipt is both faster and more crash-safe
        // than 1,000 independent directory durability barriers. A crash
        // before the one atomic write simply leaves every intent recoverable
        // on the next run; a committed batch accounts for every named orphan.
        let digest = sha256_hex(&canonical_json(&recovered)?);
        let marker = serde_json::json!({
            "schema_version": CAPSULE_SCHEMA_VERSION,
            "recovery_batch_id": digest,
            "attempt_ids": recovered,
            "status": "orphaned",
            "failure_code": "incomplete_after_recovery",
            "authority": "zero"
        });
        let marker_path = orphan_dir.join(format!("recovery.{digest}.json"));
        write_exclusive_idempotent(&marker_path, &canonical_value(&marker)?)?;
        Ok(marker["attempt_ids"].as_array().map_or(0, Vec::len))
    }

    pub fn execute(&self, request: &CaptureRequest) -> Result<ExecutionOutcome> {
        if request.owner_key != self.owner_key || request.source != self.source {
            return Err(CoreError::Integrity(
                "capture request crosses collector scope".to_string(),
            ));
        }
        let command_sha256 = sha256_hex(request.command.as_bytes());
        let attempt_id = attempt_id(request, &command_sha256);
        let intent = AttemptIntent {
            schema_version: CAPSULE_SCHEMA_VERSION,
            attempt_id: attempt_id.clone(),
            owner_key: request.owner_key.clone(),
            source: request.source.clone(),
            source_project_key: request.source_project_key.clone(),
            source_session_key: request.source_session_key.clone(),
            source_turn_id: request.source_turn_id.clone(),
            source_tool_id: request.source_tool_id.clone(),
            wrapper_nonce: request.wrapper_nonce.clone(),
            command_sha256: command_sha256.clone(),
            source_config_sha256: request.source_config_sha256.clone(),
            source_version: request.source_version.clone(),
            command: request.command.clone(),
            cwd: request.cwd.display().to_string(),
            created_at_unix_ms: unix_millis()?,
        };
        self.write_intent(&intent)?;

        let profile = command_profile(&request.command, &request.cwd)?;
        let started = Instant::now();
        let pre_start = Instant::now();
        let pre_state =
            capture_stabilized_profile(&request.cwd, &profile, self.signing_key.as_bytes());
        let pre_duration = duration_ns(pre_start.elapsed());
        let mut pre = None;
        let mut failure_code = None;
        if let Ok(state) = pre_state {
            let strength = capture_strength(request, &state);
            let payload = CapsulePayload {
                schema_version: CAPSULE_SCHEMA_VERSION,
                attempt_id: attempt_id.clone(),
                phase: CapsulePhase::Pre,
                prior_capsule_id: None,
                owner_key: request.owner_key.clone(),
                source: request.source.clone(),
                source_project_key: request.source_project_key.clone(),
                source_session_key: request.source_session_key.clone(),
                source_turn_id: request.source_turn_id.clone(),
                source_tool_id: request.source_tool_id.clone(),
                command_sha256: command_sha256.clone(),
                source_config_sha256: request.source_config_sha256.clone(),
                source_version: request.source_version.clone(),
                profile: profile.name.clone(),
                strength,
                observed_at_unix_ms: unix_millis()?,
                capture_duration_ns: pre_duration,
                state,
            };
            pre = Some(self.seal_capsule(payload)?.0);
        } else if let Err(error) = pre_state {
            failure_code = Some(format!("pre_capture_failed:{error}"));
        }

        let status = execute_shell(&request.command, &request.cwd)?;
        let ended_ns = duration_ns(started.elapsed());
        let (exit_code, term_signal) = exit_parts(&status);

        let post_start = Instant::now();
        let post_state =
            capture_stabilized_profile(&request.cwd, &profile, self.signing_key.as_bytes());
        let post_duration = duration_ns(post_start.elapsed());
        let mut post = None;
        if let Ok(state) = post_state {
            let strength = capture_strength(request, &state);
            let payload = CapsulePayload {
                schema_version: CAPSULE_SCHEMA_VERSION,
                attempt_id: attempt_id.clone(),
                phase: CapsulePhase::Post,
                prior_capsule_id: pre.as_ref().map(|capsule| capsule.capsule_id.clone()),
                owner_key: request.owner_key.clone(),
                source: request.source.clone(),
                source_project_key: request.source_project_key.clone(),
                source_session_key: request.source_session_key.clone(),
                source_turn_id: request.source_turn_id.clone(),
                source_tool_id: request.source_tool_id.clone(),
                command_sha256,
                source_config_sha256: request.source_config_sha256.clone(),
                source_version: request.source_version.clone(),
                profile: profile.name.clone(),
                strength,
                observed_at_unix_ms: unix_millis()?,
                capture_duration_ns: post_duration,
                state,
            };
            post = Some(self.seal_capsule(payload)?.0);
        } else if let Err(error) = post_state {
            failure_code = Some(format!("post_capture_failed:{error}"));
        }

        let strength = execution_strength(request, pre.as_ref(), post.as_ref());
        if strength == CaptureStrength::Unknown && failure_code.is_none() {
            failure_code = Some("capture_incomplete_or_state_changed".to_string());
        }
        let receipt = WrapperReceipt {
            schema_version: CAPSULE_SCHEMA_VERSION,
            attempt_id: attempt_id.clone(),
            pre_capsule_id: pre.as_ref().map(|capsule| capsule.capsule_id.clone()),
            post_capsule_id: post.as_ref().map(|capsule| capsule.capsule_id.clone()),
            started_monotonic_ns: 0,
            ended_monotonic_ns: ended_ns,
            exit_code,
            term_signal,
            strength,
            failure_code,
            completed_at_unix_ms: unix_millis()?,
        };
        let (receipt_sha256, _) = self.write_receipt(&receipt)?;
        let process_exit = exit_code.unwrap_or_else(|| 128 + term_signal.unwrap_or(1));
        Ok(ExecutionOutcome {
            attempt_id,
            receipt,
            receipt_sha256,
            pre,
            post,
            exit_code: process_exit,
        })
    }

    pub fn observe(&self, request: &CaptureRequest) -> Result<ObservationOutcome> {
        if request.owner_key != self.owner_key || request.source != self.source {
            return Err(CoreError::Integrity(
                "observation request crosses collector scope".to_string(),
            ));
        }
        let command_sha256 = sha256_hex(request.command.as_bytes());
        let attempt_id = attempt_id(request, &command_sha256);
        let intent = AttemptIntent {
            schema_version: CAPSULE_SCHEMA_VERSION,
            attempt_id: attempt_id.clone(),
            owner_key: request.owner_key.clone(),
            source: request.source.clone(),
            source_project_key: request.source_project_key.clone(),
            source_session_key: request.source_session_key.clone(),
            source_turn_id: request.source_turn_id.clone(),
            source_tool_id: request.source_tool_id.clone(),
            wrapper_nonce: request.wrapper_nonce.clone(),
            command_sha256: command_sha256.clone(),
            source_config_sha256: request.source_config_sha256.clone(),
            source_version: request.source_version.clone(),
            command: request.command.clone(),
            cwd: request.cwd.display().to_string(),
            created_at_unix_ms: unix_millis()?,
        };
        self.write_intent(&intent)?;
        let started = Instant::now();
        let state =
            capture_stabilized(&request.cwd, &request.command, self.signing_key.as_bytes())?;
        // An explicit observation has no wrapper-enforced execution boundary.
        // Even an otherwise complete, source-attested state is context only.
        let strength = if state.repository_id.is_some() {
            CaptureStrength::BoundedObservation
        } else {
            CaptureStrength::Unknown
        };
        let payload = CapsulePayload {
            schema_version: CAPSULE_SCHEMA_VERSION,
            attempt_id: attempt_id.clone(),
            phase: CapsulePhase::Observation,
            prior_capsule_id: None,
            owner_key: request.owner_key.clone(),
            source: request.source.clone(),
            source_project_key: request.source_project_key.clone(),
            source_session_key: request.source_session_key.clone(),
            source_turn_id: request.source_turn_id.clone(),
            source_tool_id: request.source_tool_id.clone(),
            command_sha256,
            source_config_sha256: request.source_config_sha256.clone(),
            source_version: request.source_version.clone(),
            profile: command_profile(&request.command, &request.cwd)?.name,
            strength,
            observed_at_unix_ms: unix_millis()?,
            capture_duration_ns: duration_ns(started.elapsed()),
            state,
        };
        let (capsule, path) = self.seal_capsule(payload)?;
        let relative = path.strip_prefix(&self.root).map_err(|_| {
            CoreError::Integrity("sealed observation escaped capture root".to_string())
        })?;
        Ok(ObservationOutcome {
            attempt_id,
            capsule,
            private_relative_ref: relative.to_string_lossy().to_string(),
        })
    }
}

/// Persist an exact source/version capability measurement. This is the only
/// path that can enable Codex assurance; compatible-looking Claude schemas do
/// not satisfy the source-specific controlled-execution requirement.
pub fn record_source_assurance(
    ledger: &Ledger,
    collector: &Collector,
    measurement: &SourceAssuranceMeasurement,
) -> Result<String> {
    if collector.source != measurement.source || !measurement.enables_assured() {
        return Err(CoreError::Integrity(
            "source assurance measurement did not pass every source-specific gate".to_string(),
        ));
    }
    let (measurement_sha256, signature_hex) = collector.sign_record(measurement)?;
    let measurement_json = String::from_utf8(canonical_json(measurement)?).map_err(|_| {
        CoreError::Integrity("canonical source measurement was not UTF-8".to_string())
    })?;
    let id = hash_fields(&[
        &collector.owner_key,
        &measurement.source,
        &measurement.source_version,
        &measurement.config_sha256,
        &measurement_sha256,
        "m5r1-source-assurance-v1",
    ]);
    let tx = ledger.conn().unchecked_transaction()?;
    tx.execute(
        "INSERT OR IGNORE INTO collector_keys(
           key_id,owner_key,source,algorithm,public_key_hex,created_at_unix_ms)
         VALUES(?1,?2,?3,'ed25519',?4,?5)",
        params![
            collector.key_id,
            collector.owner_key,
            collector.source,
            collector.public_key_hex(),
            unix_millis()? as i64,
        ],
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO source_assurance_attestations(
           id,owner_key,source,source_version,config_sha256,matcher,
           pre_blocks,rewrite_exact,stable_tool_identity,completion_pairing,
           measurement_json,measurement_sha256,key_id,signature_hex,accepted_at_unix_ms)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
        params![
            id,
            collector.owner_key,
            measurement.source,
            measurement.source_version,
            measurement.config_sha256,
            measurement.matcher,
            measurement.pre_blocks as i64,
            measurement.rewrite_exact as i64,
            measurement.stable_tool_identity as i64,
            measurement.completion_pairing as i64,
            measurement_json,
            measurement_sha256,
            collector.key_id,
            signature_hex,
            unix_millis()? as i64,
        ],
    )?;
    tx.commit()?;
    Ok(id)
}

pub fn source_assurance_enabled(
    ledger: &Ledger,
    owner_key: &str,
    source: &str,
    source_version: &str,
    config_sha256: &str,
) -> Result<bool> {
    if source != "codex" {
        return Ok(false);
    }
    let mut statement = ledger.conn().prepare(
        "SELECT a.measurement_json,a.measurement_sha256,a.signature_hex,k.public_key_hex
         FROM source_assurance_attestations a
         JOIN collector_keys k ON k.key_id=a.key_id
         WHERE a.owner_key=?1 AND a.source=?2 AND a.source_version=?3
           AND a.config_sha256=?4
           AND a.pre_blocks=1 AND a.rewrite_exact=1
           AND a.stable_tool_identity=1 AND a.completion_pairing=1
         ORDER BY a.accepted_at_unix_ms,a.id",
    )?;
    let rows = statement.query_map(
        params![owner_key, source, source_version, config_sha256],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        },
    )?;
    for row in rows {
        let (json, expected_digest, signature_hex, public_hex) = row?;
        let measurement: SourceAssuranceMeasurement = match serde_json::from_str(&json) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let canonical = canonical_json(&measurement)?;
        if sha256_hex(&canonical) != expected_digest
            || measurement.source != source
            || measurement.source_version != source_version
            || measurement.config_sha256 != config_sha256
            || !measurement.enables_assured()
        {
            continue;
        }
        let public = decode_hex_fixed::<32>(&public_hex)?;
        let verifying = VerifyingKey::from_bytes(&public)
            .map_err(|_| CoreError::Integrity("invalid attestation public key".to_string()))?;
        let signature = Signature::from_bytes(&decode_hex_fixed::<64>(&signature_hex)?);
        if verifying.verify(&canonical, &signature).is_ok() {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Atomically attach a complete wrapper pair to one immutable source result.
/// Any failpoint rolls back all capture rows and leaves the event ledger and
/// prior proof authority unchanged.
pub fn accept_capture_pair(
    ledger: &Ledger,
    collector: &Collector,
    acceptance: &CaptureAcceptance<'_>,
) -> Result<AcceptedCapture> {
    accept_capture_pair_with_failpoint(ledger, collector, acceptance, None)
}

/// The SINGLE accessor for a tool's vendor name.
///
/// Canonicalisation collapses five codex tools onto `Bash` and two onto `Edit`
/// (A112), so `tool_name` alone cannot key a registry. `source_tool_name` holds
/// the raw name only where canonicalisation changed it, so it alone is `NULL`
/// for unmapped tools. Every registry lookup goes through here; a call site
/// reading either column directly is a bug.
pub fn raw_tool_name<'a>(canonical: &'a str, source: Option<&'a str>) -> &'a str {
    source.unwrap_or(canonical)
}

/// What a registered tool is, for capture purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureClass {
    /// Runs a shell command whose exit status an attested run can speak to.
    Shell,
}

/// Where a registered tool's command lives in its stored `tool_input`.
///
/// One extractor per registration, no chain. A chain cannot express the
/// measured reality: `exec_command` and `shell_command` both store as `Bash`
/// but carry their commands under different keys (A112).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandExtractor {
    /// Top-level `cmd`.
    TopLevelCmd,
    /// Top-level `command`.
    TopLevelCommand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistrationStatus {
    Registered,
    /// Examined and deliberately outside assured capture.
    Excluded,
}

/// One `(vendor, RAW tool name)` decision, on the record.
#[derive(Debug, Clone, Copy)]
pub struct ToolRegistration {
    pub vendor: &'static str,
    /// The VENDOR'S name, read through [`raw_tool_name`] — never the canonical
    /// one, which collapses five codex tools onto `Bash`.
    pub raw_tool: &'static str,
    pub class: Option<CaptureClass>,
    pub extractor: Option<CommandExtractor>,
    pub status: RegistrationStatus,
    pub reason: &'static str,
}

/// The registry. **Registration is an explicit act, never inferred.**
///
/// Every row was measured before it was written, and the exclusions cite the
/// stored rows that justify them — not rollout counts, which is the mistake
/// A108 made. Exclusions are ROWS, not absences, so "we have not looked at
/// this" and "we looked and it cannot work" stay distinguishable.
///
/// Deliberately absent, and now *expressibly* absent because A113 preserves the
/// raw name: `bash`, `shell`, `terminal`. All three canonicalise to `Bash` and
/// all three are plausible shell tools, but none has been measured in a real
/// corpus, so none is registered. Before A113 they were indistinguishable from
/// `exec_command` in storage and could not have been left out on purpose.
pub const TOOL_REGISTRY: &[ToolRegistration] = &[
    ToolRegistration {
        vendor: "codex",
        raw_tool: "exec_command",
        class: Some(CaptureClass::Shell),
        extractor: Some(CommandExtractor::TopLevelCmd),
        status: RegistrationStatus::Registered,
        reason: "A112: stores tool_name=Bash, source_tool_name=exec_command, \
                 tool_input={\"cmd\":…}",
    },
    ToolRegistration {
        vendor: "codex",
        raw_tool: "shell_command",
        class: Some(CaptureClass::Shell),
        extractor: Some(CommandExtractor::TopLevelCommand),
        status: RegistrationStatus::Registered,
        reason: "A112: stores tool_name=Bash, source_tool_name=shell_command, \
                 tool_input={\"command\":…}",
    },
    ToolRegistration {
        vendor: "claude_code",
        raw_tool: "Bash",
        class: Some(CaptureClass::Shell),
        extractor: Some(CommandExtractor::TopLevelCommand),
        status: RegistrationStatus::Registered,
        reason: "the Claude Code shell tool; not canonicalised, so raw and canonical agree",
    },
    ToolRegistration {
        vendor: "codex",
        raw_tool: "exec",
        class: None,
        extractor: None,
        status: RegistrationStatus::Excluded,
        reason: "A112 row-receipt: stores tool_input=null — the call carries no command, \
                 so nothing can be bound to a run",
    },
    ToolRegistration {
        vendor: "codex",
        raw_tool: "apply_patch",
        class: None,
        extractor: None,
        status: RegistrationStatus::Excluded,
        reason: "A112 row-receipt: stores tool_name=Edit, tool_input=null; a patch \
                 application has no command whose exit status speaks to a claim",
    },
    ToolRegistration {
        vendor: "codex",
        raw_tool: "write_stdin",
        class: None,
        extractor: None,
        status: RegistrationStatus::Excluded,
        reason: "A112 row-receipt: stores tool_input={\"chars\":…} — writes to an existing \
                 process's stdin, so it has no exit status of its own",
    },
];

pub fn registration(vendor: &str, raw_tool: &str) -> Option<&'static ToolRegistration> {
    TOOL_REGISTRY
        .iter()
        .find(|r| r.vendor == vendor && r.raw_tool == raw_tool)
}

/// Read a registered tool's command. **No fallback.**
///
/// An unregistered pair returns `None` because nobody registered it — not
/// because a shape went unrecognised, and the two are different facts. An
/// excluded pair returns `None` for a reason already on the record.
pub fn registered_command(vendor: &str, raw_tool: &str, tool_input: &str) -> Option<String> {
    let registration = registration(vendor, raw_tool)?;
    if registration.status != RegistrationStatus::Registered {
        return None;
    }
    match registration.extractor? {
        CommandExtractor::TopLevelCmd => crate::derive::json_string_field(tool_input, "cmd"),
        CommandExtractor::TopLevelCommand => {
            crate::derive::json_string_field(tool_input, "command")
        }
    }
}

fn exact_source_command_match(
    ledger: &Ledger,
    vendor: &str,
    session_id: &str,
    source_turn_id: &str,
    result_tool_id: &str,
    command_sha256: &str,
) -> Result<bool> {
    let mut statement = ledger.conn().prepare(
        "SELECT COALESCE(tool_use_id,''),COALESCE(tool_input,''),
                COALESCE(tool_name,''),source_tool_name
         FROM events
         WHERE session_id=?1 AND source_turn_id=?2 AND kind='tool_use'
         ORDER BY seq,id",
    )?;
    let rows = statement.query_map(params![session_id, source_turn_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
        ))
    })?;
    let mut matching = Vec::new();
    for row in rows {
        let (tool_id, input, canonical, raw) = row?;
        if registered_command(vendor, raw_tool_name(&canonical, raw.as_deref()), &input)
            .as_deref()
            .map(|command| sha256_hex(command.as_bytes()) == command_sha256)
            .unwrap_or(false)
        {
            matching.push(tool_id);
        }
    }
    Ok(matching.len() == 1 && matching[0] == result_tool_id)
}

struct AcceptedSourceEvent {
    session_id: String,
    kind: String,
    tool_id: String,
    owner: String,
    project_key: String,
    session_key: String,
    turn_id: Option<String>,
}

pub fn accept_capture_pair_with_failpoint(
    ledger: &Ledger,
    collector: &Collector,
    acceptance: &CaptureAcceptance<'_>,
    failpoint: Option<CaptureCommitFailpoint>,
) -> Result<AcceptedCapture> {
    let outcome = acceptance.outcome;
    let delivery = acceptance.delivery;
    let (Some(pre), Some(post)) = (outcome.pre.as_ref(), outcome.post.as_ref()) else {
        return Err(CoreError::Integrity(
            "capture pair is missing PRE or POST".to_string(),
        ));
    };
    collector.verify_capsule(pre)?;
    collector.verify_capsule(post)?;
    if outcome.attempt_id != delivery.attempt_id
        || outcome.attempt_id != pre.payload.attempt_id
        || outcome.attempt_id != post.payload.attempt_id
        || delivery.owner_key != collector.owner_key
        || delivery.source != collector.source
        || delivery.source_project_key != pre.payload.source_project_key
        || delivery.source_session_key != pre.payload.source_session_key
        || delivery.source_turn_id != pre.payload.source_turn_id
        || delivery.source_tool_id != pre.payload.source_tool_id
        || post.payload.prior_capsule_id.as_deref() != Some(pre.capsule_id.as_str())
        || pre.payload.phase != CapsulePhase::Pre
        || post.payload.phase != CapsulePhase::Post
        || pre.payload.state != post.payload.state
        || outcome.receipt.pre_capsule_id.as_deref() != Some(pre.capsule_id.as_str())
        || outcome.receipt.post_capsule_id.as_deref() != Some(post.capsule_id.as_str())
        || outcome.receipt.exit_code != delivery.source_exit_code
        || outcome.receipt.term_signal != delivery.source_term_signal
    {
        return Err(CoreError::Integrity(
            "wrapper/source delivery pair disagrees".to_string(),
        ));
    }
    let event: Option<AcceptedSourceEvent> = ledger
        .conn()
        .query_row(
            "SELECT e.project_id,e.session_id,e.kind,COALESCE(e.tool_use_id,''),
                    spi.owner_key,spi.source_project_key,ss.source_session_key,
                    e.source_turn_id
             FROM events e
             JOIN projects p ON p.id=e.project_id
             JOIN source_project_identities spi ON spi.project_id=e.project_id
             JOIN source_sessions ss ON ss.session_id=e.session_id
             WHERE e.id=?1 AND p.source=?2",
            params![delivery.source_event_id, delivery.source],
            |row| {
                Ok(AcceptedSourceEvent {
                    session_id: row.get(1)?,
                    kind: row.get(2)?,
                    tool_id: row.get(3)?,
                    owner: row.get(4)?,
                    project_key: row.get(5)?,
                    session_key: row.get(6)?,
                    turn_id: row.get(7)?,
                })
            },
        )
        .optional()?;
    let Some(event) = event else {
        return Err(CoreError::Integrity(
            "source delivery event does not exist in the accepted ledger".to_string(),
        ));
    };
    let project_identity_matches = event.project_key == delivery.source_project_key
        || opaque_source_project_key(&event.owner, &delivery.source, &event.project_key)
            == delivery.source_project_key;
    let exact_turn_and_command = delivery
        .source_turn_id
        .as_deref()
        .zip(event.turn_id.as_deref())
        .filter(|(delivery_turn, event_turn)| delivery_turn == event_turn)
        .map(|(turn, _)| {
            exact_source_command_match(
                ledger,
                &delivery.source,
                &event.session_id,
                turn,
                &event.tool_id,
                &pre.payload.command_sha256,
            )
        })
        .transpose()?
        .unwrap_or(false);
    if event.kind != "tool_result"
        || event.owner != delivery.owner_key
        || !project_identity_matches
        || event.session_key != delivery.source_session_key
        || !exact_turn_and_command
    {
        return Err(CoreError::Integrity(
            "source delivery does not match event identity".to_string(),
        ));
    }
    let exact_attestation = source_assurance_enabled(
        ledger,
        &collector.owner_key,
        &collector.source,
        &pre.payload.source_version,
        &pre.payload.source_config_sha256,
    )?;
    let final_strength = if outcome.receipt.strength == CaptureStrength::Assured
        && collector.source == "codex"
        && exact_attestation
        && pre.payload.state.completeness.complete()
        && post.payload.state.completeness.complete()
    {
        CaptureStrength::Assured
    } else if pre.payload.state.repository_id.is_some() {
        CaptureStrength::BoundedObservation
    } else {
        CaptureStrength::Unknown
    };
    validate_private_ref(&acceptance.pre_private_relative_ref, &outcome.attempt_id)?;
    validate_private_ref(&acceptance.post_private_relative_ref, &outcome.attempt_id)?;
    collector.verify_private_capsule_ref(&acceptance.pre_private_relative_ref, pre)?;
    collector.verify_private_capsule_ref(&acceptance.post_private_relative_ref, post)?;
    let receipt_path = collector
        .root
        .join("sealed")
        .join(&outcome.attempt_id)
        .join(format!("receipt.{}.json", outcome.receipt_sha256));
    let receipt_bytes = read_bounded(&receipt_path, 64 * 1024)?;
    if sha256_hex(&receipt_bytes) != outcome.receipt_sha256
        || serde_json::from_slice::<WrapperReceipt>(&receipt_bytes)? != outcome.receipt
    {
        return Err(CoreError::Integrity(
            "private wrapper receipt failed verification".to_string(),
        ));
    }
    let delivery_sha256 = sha256_hex(&canonical_json(delivery)?);
    let now = unix_millis()? as i64;
    let tx = ledger.conn().unchecked_transaction()?;
    tx.execute(
        "INSERT OR IGNORE INTO collector_keys(
           key_id,owner_key,source,algorithm,public_key_hex,created_at_unix_ms)
         VALUES(?1,?2,?3,'ed25519',?4,?5)",
        params![
            collector.key_id,
            collector.owner_key,
            collector.source,
            collector.public_key_hex(),
            now,
        ],
    )?;
    let inserted = tx.execute(
        "INSERT OR IGNORE INTO capture_attempts(
           attempt_id,owner_key,source,source_project_key,source_session_key,
           source_turn_id,source_tool_id,wrapper_nonce,command_sha256,source_config_sha256,
           mechanism,source_version,profile,strength,status,failure_code,
           started_monotonic_ns,ended_monotonic_ns,exit_code,term_signal,
           receipt_sha256,created_at_unix_ms)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'native_pre_wrapper_post',
                ?11,?12,?13,'accepted',NULL,?14,?15,?16,?17,?18,?19)",
        params![
            outcome.attempt_id,
            collector.owner_key,
            collector.source,
            delivery.source_project_key,
            delivery.source_session_key,
            delivery.source_turn_id,
            delivery.source_tool_id,
            delivery.wrapper_nonce,
            pre.payload.command_sha256,
            pre.payload.source_config_sha256,
            pre.payload.source_version,
            pre.payload.profile,
            final_strength.as_str(),
            outcome.receipt.started_monotonic_ns as i64,
            outcome.receipt.ended_monotonic_ns as i64,
            outcome.receipt.exit_code,
            outcome.receipt.term_signal,
            outcome.receipt_sha256,
            now,
        ],
    )?;
    if inserted == 0 {
        let stored: String = tx.query_row(
            "SELECT receipt_sha256 FROM capture_attempts WHERE attempt_id=?1",
            params![outcome.attempt_id],
            |row| row.get(0),
        )?;
        if stored != outcome.receipt_sha256 {
            return Err(CoreError::Integrity(
                "conflicting duplicate capture attempt".to_string(),
            ));
        }
    }
    fail_capture_commit(failpoint, CaptureCommitFailpoint::AfterAttempt)?;
    let next_seq: i64 = tx.query_row(
        "SELECT COALESCE(MAX(accepted_seq),0)+1 FROM state_capsules",
        [],
        |row| row.get(0),
    )?;
    insert_capsule_row(
        &tx,
        pre,
        &acceptance.pre_private_relative_ref,
        next_seq,
        now,
    )?;
    fail_capture_commit(failpoint, CaptureCommitFailpoint::AfterPre)?;
    insert_capsule_row(
        &tx,
        post,
        &acceptance.post_private_relative_ref,
        next_seq + 1,
        now,
    )?;
    fail_capture_commit(failpoint, CaptureCommitFailpoint::AfterPost)?;
    fail_capture_commit(failpoint, CaptureCommitFailpoint::BeforeLink)?;
    let link_id = hash_fields(&[
        &outcome.attempt_id,
        &delivery.source_event_id,
        &delivery_sha256,
        "m5r1-capture-event-link-v1",
    ]);
    tx.execute(
        "INSERT OR IGNORE INTO capture_event_links(
           id,attempt_id,event_id,source_tool_id,delivery_sha256,linked_at_unix_ms)
         VALUES(?1,?2,?3,?4,?5,?6)",
        params![
            link_id,
            outcome.attempt_id,
            delivery.source_event_id,
            delivery.source_tool_id,
            delivery_sha256,
            now,
        ],
    )?;
    fail_capture_commit(failpoint, CaptureCommitFailpoint::BeforeCommit)?;
    tx.commit()?;
    Ok(AcceptedCapture {
        attempt_id: outcome.attempt_id.clone(),
        event_id: delivery.source_event_id.clone(),
        pre_capsule_id: pre.capsule_id.clone(),
        post_capsule_id: post.capsule_id.clone(),
        strength: final_strength,
    })
}

fn insert_capsule_row(
    tx: &rusqlite::Transaction<'_>,
    capsule: &SignedCapsule,
    private_ref: &str,
    accepted_seq: i64,
    accepted_at: i64,
) -> Result<()> {
    let state = &capsule.payload.state;
    tx.execute(
        "INSERT OR IGNORE INTO state_capsules(
           capsule_id,attempt_id,phase,schema_version,prior_capsule_id,
           canonical_sha256,signature_hex,key_id,private_relative_ref,complete,
           completeness_json,repository_id,common_dir_id,worktree_id,
           platform_id,head_ref_digest,detached,operation_state_digest,
           head_commit,head_tree,index_digest,
           tracked_worktree_digest,staged_digest,untracked_digest,
           toolchain_digest,lock_config_digest,environment_digest,profile_digest,
           accepted_seq,accepted_at_unix_ms)
         VALUES(?1,?2,?3,?4,?5,?1,?6,?7,?8,?9,?10,?11,?12,?13,?14,
                ?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26,?27,
                ?28,?29)",
        params![
            capsule.capsule_id,
            capsule.payload.attempt_id,
            capsule.payload.phase.as_str(),
            capsule.payload.schema_version as i64,
            capsule.payload.prior_capsule_id,
            capsule.signature_hex,
            capsule.key_id,
            private_ref,
            state.completeness.complete() as i64,
            serde_json::to_string(&state.completeness)?,
            state.repository_id,
            state.common_dir_id,
            state.worktree_id,
            state.platform_id,
            state.head_ref_digest,
            state.detached as i64,
            state.operation_state_digest,
            state.head_commit,
            state.head_tree,
            state.index_digest,
            state.tracked_worktree_digest,
            state.staged_digest,
            state.untracked_digest,
            state.toolchain_digest,
            state.lock_config_digest,
            state.environment_digest,
            state.profile_digest,
            accepted_seq,
            accepted_at,
        ],
    )?;
    Ok(())
}

pub fn record_state_comparison(
    ledger: &Ledger,
    execution_capsule_id: &str,
    observation_capsule_id: &str,
    comparison: &Comparison,
) -> Result<String> {
    let execution = load_capsule_state(ledger, execution_capsule_id)?;
    let observation = load_capsule_state(ledger, observation_capsule_id)?;
    let derived = if execution.phase != "post"
        || observation.phase != "observation"
        || execution.owner_key != observation.owner_key
        || execution.source != observation.source
        || execution.source_project_key != observation.source_project_key
        || execution.strength != "assured"
        || observation.strength != "bounded_observation"
    {
        Comparison {
            currentness: ProofApplicability::Unknown,
            comparable_dimensions: Vec::new(),
            changed_dimensions: Vec::new(),
            reason: "capture mechanisms or source lineage are not comparable".to_string(),
        }
    } else {
        compare_states(&execution.state, &observation.state)
    };
    if &derived != comparison {
        return Err(CoreError::Integrity(
            "caller-supplied comparison disagrees with accepted capsule rows".to_string(),
        ));
    }
    let comparison_id = hash_fields(&[
        execution_capsule_id,
        observation_capsule_id,
        comparison.currentness.as_str(),
        &comparison.changed_dimensions.join(","),
        "m5r1-state-comparison-v1",
    ]);
    let next_seq: i64 = ledger.conn().query_row(
        "SELECT COALESCE(MAX(accepted_seq),0)+1 FROM state_comparisons",
        [],
        |row| row.get(0),
    )?;
    ledger.conn().execute(
        "INSERT OR IGNORE INTO state_comparisons(
           comparison_id,execution_capsule_id,observation_capsule_id,currentness,
           comparable_dimensions_json,changed_dimensions_json,reason,
           accepted_seq,accepted_at_unix_ms)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![
            comparison_id,
            execution_capsule_id,
            observation_capsule_id,
            comparison.currentness.as_str(),
            serde_json::to_string(&comparison.comparable_dimensions)?,
            serde_json::to_string(&comparison.changed_dimensions)?,
            comparison.reason,
            next_seq,
            unix_millis()? as i64,
        ],
    )?;
    Ok(comparison_id)
}

struct StoredCapsuleState {
    phase: String,
    owner_key: String,
    source: String,
    source_project_key: String,
    strength: String,
    state: StateFingerprint,
}

fn load_capsule_state(ledger: &Ledger, capsule_id: &str) -> Result<StoredCapsuleState> {
    ledger
        .conn()
        .query_row(
            "SELECT s.phase,a.owner_key,a.source,a.source_project_key,a.strength,
                    s.repository_id,s.common_dir_id,s.worktree_id,s.platform_id,
                    s.head_ref_digest,s.detached,s.operation_state_digest,
                    s.head_commit,s.head_tree,s.index_digest,s.tracked_worktree_digest,
                    s.staged_digest,s.untracked_digest,s.toolchain_digest,
                    s.lock_config_digest,s.environment_digest,s.profile_digest,
                    s.completeness_json
             FROM state_capsules s JOIN capture_attempts a ON a.attempt_id=s.attempt_id
             WHERE s.capsule_id=?1",
            params![capsule_id],
            |row| {
                let completeness_json: String = row.get(22)?;
                let completeness: Completeness =
                    serde_json::from_str(&completeness_json).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            22,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?;
                Ok(StoredCapsuleState {
                    phase: row.get(0)?,
                    owner_key: row.get(1)?,
                    source: row.get(2)?,
                    source_project_key: row.get(3)?,
                    strength: row.get(4)?,
                    state: StateFingerprint {
                        repository_id: row.get(5)?,
                        common_dir_id: row.get(6)?,
                        worktree_id: row.get(7)?,
                        platform_id: row.get(8)?,
                        head_ref_digest: row.get(9)?,
                        detached: row.get::<_, i64>(10)? != 0,
                        operation_state_digest: row.get(11)?,
                        head_commit: row.get(12)?,
                        head_tree: row.get(13)?,
                        index_digest: row.get(14)?,
                        tracked_worktree_digest: row.get(15)?,
                        staged_digest: row.get(16)?,
                        untracked_digest: row.get(17)?,
                        toolchain_digest: row.get(18)?,
                        lock_config_digest: row.get(19)?,
                        environment_digest: row.get(20)?,
                        profile_digest: row.get(21)?,
                        tracked_files: 0,
                        untracked_files: 0,
                        hashed_bytes: 0,
                        completeness,
                    },
                })
            },
        )
        .optional()?
        .ok_or_else(|| CoreError::Integrity(format!("capsule {capsule_id} is not accepted")))
}

pub fn accept_observation(
    ledger: &Ledger,
    collector: &Collector,
    observation: &ObservationOutcome,
) -> Result<()> {
    let capsule = &observation.capsule;
    collector.verify_capsule(capsule)?;
    if capsule.payload.phase != CapsulePhase::Observation
        || capsule.payload.attempt_id != observation.attempt_id
    {
        return Err(CoreError::Integrity(
            "observation capsule identity disagrees".to_string(),
        ));
    }
    validate_private_ref(&observation.private_relative_ref, &observation.attempt_id)?;
    let strength = if capsule.payload.state.repository_id.is_some() {
        CaptureStrength::BoundedObservation
    } else {
        CaptureStrength::Unknown
    };
    let now = unix_millis()? as i64;
    let tx = ledger.conn().unchecked_transaction()?;
    tx.execute(
        "INSERT OR IGNORE INTO collector_keys(
           key_id,owner_key,source,algorithm,public_key_hex,created_at_unix_ms)
         VALUES(?1,?2,?3,'ed25519',?4,?5)",
        params![
            collector.key_id,
            collector.owner_key,
            collector.source,
            collector.public_key_hex(),
            now,
        ],
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO capture_attempts(
           attempt_id,owner_key,source,source_project_key,source_session_key,
           source_turn_id,source_tool_id,wrapper_nonce,command_sha256,source_config_sha256,
           mechanism,source_version,profile,strength,status,failure_code,
           receipt_sha256,created_at_unix_ms)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'explicit_observation',
                ?11,?12,?13,'accepted',NULL,NULL,?14)",
        params![
            observation.attempt_id,
            collector.owner_key,
            collector.source,
            capsule.payload.source_project_key,
            capsule.payload.source_session_key,
            capsule.payload.source_turn_id,
            capsule.payload.source_tool_id,
            observation.attempt_id,
            capsule.payload.command_sha256,
            capsule.payload.source_config_sha256,
            capsule.payload.source_version,
            capsule.payload.profile,
            strength.as_str(),
            now,
        ],
    )?;
    let next_seq: i64 = tx.query_row(
        "SELECT COALESCE(MAX(accepted_seq),0)+1 FROM state_capsules",
        [],
        |row| row.get(0),
    )?;
    insert_capsule_row(
        &tx,
        capsule,
        &observation.private_relative_ref,
        next_seq,
        now,
    )?;
    tx.commit()?;
    Ok(())
}

/// Atomically accept one explicit observation and publish every intended
/// comparison against assured POST capsules in the same owner/source/project
/// lineage. A failed transaction makes neither the observation nor any
/// comparison visible; retries are idempotent.
pub fn accept_observation_and_publish(
    ledger: &Ledger,
    collector: &Collector,
    observation: &ObservationOutcome,
) -> Result<ObservationPublication> {
    let capsule = &observation.capsule;
    collector.verify_capsule(capsule)?;
    if capsule.payload.phase != CapsulePhase::Observation
        || capsule.payload.attempt_id != observation.attempt_id
    {
        return Err(CoreError::Integrity(
            "observation capsule identity disagrees".to_string(),
        ));
    }
    validate_private_ref(&observation.private_relative_ref, &observation.attempt_id)?;

    let mut statement = ledger.conn().prepare(
        "SELECT s.capsule_id FROM state_capsules s
         JOIN capture_attempts a ON a.attempt_id=s.attempt_id
         WHERE s.phase='post' AND a.status='accepted' AND a.strength='assured'
           AND a.owner_key=?1 AND a.source=?2 AND a.source_project_key=?3
         ORDER BY s.accepted_seq",
    )?;
    let execution_ids: Vec<String> = statement
        .query_map(
            params![
                capsule.payload.owner_key,
                capsule.payload.source,
                capsule.payload.source_project_key
            ],
            |row| row.get(0),
        )?
        .collect::<std::result::Result<_, _>>()?;
    drop(statement);
    let mut comparisons = Vec::new();
    for execution_id in &execution_ids {
        let execution = load_capsule_state(ledger, execution_id)?;
        comparisons.push((
            execution_id.clone(),
            compare_states(&execution.state, &capsule.payload.state),
        ));
    }

    let strength = if capsule.payload.state.repository_id.is_some() {
        CaptureStrength::BoundedObservation
    } else {
        CaptureStrength::Unknown
    };
    let now = unix_millis()? as i64;
    let tx = ledger.conn().unchecked_transaction()?;
    tx.execute(
        "INSERT OR IGNORE INTO collector_keys(
           key_id,owner_key,source,algorithm,public_key_hex,created_at_unix_ms)
         VALUES(?1,?2,?3,'ed25519',?4,?5)",
        params![
            collector.key_id,
            collector.owner_key,
            collector.source,
            collector.public_key_hex(),
            now,
        ],
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO capture_attempts(
           attempt_id,owner_key,source,source_project_key,source_session_key,
           source_turn_id,source_tool_id,wrapper_nonce,command_sha256,source_config_sha256,
           mechanism,source_version,profile,strength,status,failure_code,
           receipt_sha256,created_at_unix_ms)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'explicit_observation',
                ?11,?12,?13,'accepted',NULL,NULL,?14)",
        params![
            observation.attempt_id,
            collector.owner_key,
            collector.source,
            capsule.payload.source_project_key,
            capsule.payload.source_session_key,
            capsule.payload.source_turn_id,
            capsule.payload.source_tool_id,
            observation.attempt_id,
            capsule.payload.command_sha256,
            capsule.payload.source_config_sha256,
            capsule.payload.source_version,
            capsule.payload.profile,
            strength.as_str(),
            now,
        ],
    )?;
    let capsule_exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM state_capsules WHERE capsule_id=?1)",
        params![capsule.capsule_id],
        |row| row.get::<_, i64>(0),
    )? != 0;
    if !capsule_exists {
        let next_seq: i64 = tx.query_row(
            "SELECT COALESCE(MAX(accepted_seq),0)+1 FROM state_capsules",
            [],
            |row| row.get(0),
        )?;
        insert_capsule_row(
            &tx,
            capsule,
            &observation.private_relative_ref,
            next_seq,
            now,
        )?;
    }
    let mut current_count = 0u64;
    let mut stale_count = 0u64;
    let mut unknown_count = 0u64;
    for (execution_id, comparison) in &comparisons {
        match comparison.currentness {
            ProofApplicability::Current => current_count += 1,
            ProofApplicability::Stale => stale_count += 1,
            ProofApplicability::Unknown => unknown_count += 1,
        }
        let comparison_id = hash_fields(&[
            execution_id,
            &capsule.capsule_id,
            comparison.currentness.as_str(),
            &comparison.changed_dimensions.join(","),
            "m5r1-state-comparison-v1",
        ]);
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM state_comparisons WHERE comparison_id=?1)",
            params![comparison_id],
            |row| row.get::<_, i64>(0),
        )? != 0;
        if !exists {
            let next_seq: i64 = tx.query_row(
                "SELECT COALESCE(MAX(accepted_seq),0)+1 FROM state_comparisons",
                [],
                |row| row.get(0),
            )?;
            tx.execute(
                "INSERT INTO state_comparisons(
                   comparison_id,execution_capsule_id,observation_capsule_id,currentness,
                   comparable_dimensions_json,changed_dimensions_json,reason,
                   accepted_seq,accepted_at_unix_ms)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![
                    comparison_id,
                    execution_id,
                    capsule.capsule_id,
                    comparison.currentness.as_str(),
                    serde_json::to_string(&comparison.comparable_dimensions)?,
                    serde_json::to_string(&comparison.changed_dimensions)?,
                    comparison.reason,
                    next_seq,
                    now,
                ],
            )?;
        }
    }
    let refresh_id = hash_fields(&[
        &collector.owner_key,
        &collector.source,
        &capsule.payload.source_project_key,
        &capsule.capsule_id,
        "m5r2-refresh-publication-v1",
    ]);
    let refresh_exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM capture_refresh_receipts WHERE refresh_id=?1)",
        params![refresh_id],
        |row| row.get::<_, i64>(0),
    )? != 0;
    if !refresh_exists {
        let seq: i64 = tx.query_row(
            "SELECT COALESCE(MAX(accepted_seq),0)+1 FROM capture_refresh_receipts",
            [],
            |row| row.get(0),
        )?;
        tx.execute(
            "INSERT INTO capture_refresh_receipts(
               refresh_id,owner_key,source,source_project_key,observation_capsule_id,
               execution_count,comparison_count,current_count,stale_count,unknown_count,
               status,reason,accepted_seq,accepted_at_unix_ms)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'published',NULL,?11,?12)",
            params![
                refresh_id,
                collector.owner_key,
                collector.source,
                capsule.payload.source_project_key,
                capsule.capsule_id,
                execution_ids.len() as i64,
                comparisons.len() as i64,
                current_count as i64,
                stale_count as i64,
                unknown_count as i64,
                seq,
                now,
            ],
        )?;
    }
    tx.commit()?;
    Ok(ObservationPublication {
        refresh_id,
        observation_capsule_id: capsule.capsule_id.clone(),
        execution_count: execution_ids.len() as u64,
        comparison_count: comparisons.len() as u64,
        current_count,
        stale_count,
        unknown_count,
    })
}

pub fn latest_proof_currentness(ledger: &Ledger, event_id: &str) -> Result<ProofApplicability> {
    let value: Option<String> = ledger
        .conn()
        .query_row(
            "SELECT c.currentness FROM capture_event_links l
             JOIN state_capsules s ON s.attempt_id=l.attempt_id AND s.phase='post'
             JOIN state_comparisons c ON c.execution_capsule_id=s.capsule_id
             WHERE l.event_id=?1 ORDER BY c.accepted_seq DESC LIMIT 1",
            params![event_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(match value.as_deref() {
        Some("current") => ProofApplicability::Current,
        Some("stale") => ProofApplicability::Stale,
        _ => ProofApplicability::Unknown,
    })
}

fn validate_private_ref(relative: &str, attempt_id: &str) -> Result<()> {
    let path = Path::new(relative);
    validate_relative_path(path)?;
    if !relative.starts_with(&format!("sealed/{attempt_id}/")) {
        return Err(CoreError::Integrity(
            "private capsule reference is outside its attempt scope".to_string(),
        ));
    }
    Ok(())
}

fn validate_opaque_project_key(value: &str) -> Result<()> {
    if value.is_empty() || Path::new(value).is_absolute() || value.contains("/../") {
        return Err(CoreError::Integrity(
            "capture project identity must be opaque, never a raw absolute path".to_string(),
        ));
    }
    Ok(())
}

fn fail_capture_commit(
    selected: Option<CaptureCommitFailpoint>,
    point: CaptureCommitFailpoint,
) -> Result<()> {
    if selected == Some(point) {
        return Err(CoreError::Integrity(format!(
            "injected capture transaction failure at {point:?}"
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommandProfile {
    name: String,
    executable: Option<PathBuf>,
    allowed_env: Vec<&'static str>,
    recognized: bool,
    failure: Option<String>,
}

fn command_profile(command: &str, cwd: &Path) -> Result<CommandProfile> {
    if command.is_empty()
        || command.contains(['\0', '\n', '\r', '&', '|', ';', '>', '<', '`'])
        || command.contains("$(")
    {
        return Ok(CommandProfile {
            name: "unrecognized_shell".to_string(),
            executable: None,
            allowed_env: Vec::new(),
            recognized: false,
            failure: Some("unsupported_shell_semantics".to_string()),
        });
    }
    let token = command
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_matches(|c| matches!(c, '\'' | '"'));
    let base = Path::new(token)
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("");
    let (name, allowed_env): (&str, Vec<&'static str>) = match base {
        "git" => ("git_v1", vec!["GIT_OPTIONAL_LOCKS", "CI"]),
        "cargo" | "rustc" => (
            "rust_v1",
            vec!["RUSTUP_TOOLCHAIN", "RUSTFLAGS", "CARGO_TARGET_DIR", "CI"],
        ),
        "node" | "npm" | "pnpm" | "yarn" => ("node_v1", vec!["NODE_ENV", "NODE_OPTIONS", "CI"]),
        _ => {
            return Ok(CommandProfile {
                name: "unrecognized_command".to_string(),
                executable: None,
                allowed_env: Vec::new(),
                recognized: false,
                failure: Some("unrecognized_tool_profile".to_string()),
            });
        }
    };
    if base == "git" {
        let lowered: Vec<String> = command
            .split_whitespace()
            .skip(1)
            .map(|part| part.trim_matches(['\'', '"']).to_ascii_lowercase())
            .collect();
        let forbidden = lowered.iter().any(|part| {
            part == "-c"
                || part.starts_with("alias.")
                || part == "--config-env"
                || matches!(
                    part.as_str(),
                    "clone" | "fetch" | "pull" | "push" | "ls-remote" | "submodule"
                )
        });
        if forbidden {
            return Ok(CommandProfile {
                name: "git_unproven_semantics".to_string(),
                executable: None,
                allowed_env: Vec::new(),
                recognized: false,
                failure: Some("unsupported_git_semantics".to_string()),
            });
        }
    }
    let executable = resolve_executable(token)?;
    let recognized = executable
        .as_ref()
        .is_some_and(|path| !path.starts_with(cwd));
    Ok(CommandProfile {
        name: name.to_string(),
        executable,
        allowed_env,
        recognized,
        failure: (!recognized).then(|| "untrusted_or_missing_executable".to_string()),
    })
}

pub fn capture_stabilized(
    cwd: &Path,
    profile_command: &str,
    keyed_hash_secret: &[u8; 32],
) -> Result<StateFingerprint> {
    let profile = command_profile(profile_command, cwd)?;
    capture_stabilized_profile(cwd, &profile, keyed_hash_secret)
}

fn capture_stabilized_profile(
    cwd: &Path,
    profile: &CommandProfile,
    keyed_hash_secret: &[u8; 32],
) -> Result<StateFingerprint> {
    let mut prior: Option<StateFingerprint> = None;
    for _ in 0..MAX_STABILIZATION_ATTEMPTS {
        let mut first = capture_once(cwd, profile, keyed_hash_secret)?;
        let second = capture_once(cwd, profile, keyed_hash_secret)?;
        if first == second {
            first.completeness.stabilized = true;
            return Ok(first);
        }
        prior = Some(second);
    }
    let mut final_state = prior.ok_or_else(|| {
        CoreError::Integrity("stabilization completed no capture pass".to_string())
    })?;
    final_state.completeness.stabilized = false;
    final_state.completeness.fail("capture_unstable");
    Err(CoreError::Integrity("capture_unstable".to_string()))
}

fn capture_once(
    cwd: &Path,
    profile: &CommandProfile,
    keyed_hash_secret: &[u8; 32],
) -> Result<StateFingerprint> {
    let mut completeness = Completeness::new();
    completeness.recognized_profile = profile.recognized;
    if let Some(failure) = &profile.failure {
        completeness.fail(failure);
    }
    let git = trusted_git()?;
    let root = match repository_root(git_output(&git, cwd, &["rev-parse", "--show-toplevel"]))? {
        Some(root) => {
            completeness.git_repository = true;
            root
        }
        None => {
            completeness.fail("non_git_directory");
            return Ok(empty_fingerprint(completeness));
        }
    };
    let root = fs::canonicalize(&root).map_err(|e| CoreError::io(&root, e))?;
    let bare = trim_output(&git_output(
        &git,
        &root,
        &["rev-parse", "--is-bare-repository"],
    )?)?;
    if bare == "true" {
        completeness.fail("bare_repository");
    } else {
        completeness.ordinary_or_linked_worktree = true;
    }
    let git_dir = resolve_git_path(
        &root,
        &git_output(&git, &root, &["rev-parse", "--git-dir"])?,
    )?;
    let common_dir = resolve_git_path(
        &root,
        &git_output(&git, &root, &["rev-parse", "--git-common-dir"])?,
    )?;
    let head_commit = git_optional(&git, &root, &["rev-parse", "--verify", "HEAD^{commit}"])?;
    let head_tree = git_optional(&git, &root, &["rev-parse", "--verify", "HEAD^{tree}"])?;
    completeness.committed_head = head_commit.is_some() && head_tree.is_some();
    if !completeness.committed_head {
        completeness.fail("unborn_head");
    }
    let head_ref = git_optional(&git, &root, &["symbolic-ref", "-q", "HEAD"])?;
    let detached = head_ref.is_none() && head_commit.is_some();
    let head_ref_digest = Some(hash_fields(&[
        head_ref.as_deref().unwrap_or("detached"),
        if detached { "detached" } else { "attached" },
    ]));

    let index_bytes = git_output(&git, &root, &["ls-files", "--stage", "-z"])?;
    let entries = parse_index_entries(&index_bytes)?;
    completeness.index = true;
    if entries.len() > MAX_TRACKED_FILES {
        completeness.fail("tracked_file_limit");
    }
    let no_submodule = !entries.iter().any(|entry| entry.mode == "160000");
    completeness.no_submodule = no_submodule;
    if !no_submodule {
        completeness.fail("submodule_present");
    }

    let sparse = git_optional(&git, &root, &["config", "--bool", "core.sparseCheckout"])?
        .is_some_and(|value| value == "true");
    completeness.no_sparse_checkout = !sparse;
    if sparse {
        completeness.fail("sparse_checkout");
    }
    completeness.no_nested_repository = !contains_nested_git(&root)?;
    if !completeness.no_nested_repository {
        completeness.fail("nested_repository");
    }

    let index_digest = sha256_hex(&index_bytes);
    let staged_digest = hash_fields(&[head_tree.as_deref().unwrap_or("unborn"), &index_digest]);
    let (tracked_worktree_digest, tracked_bytes) =
        hash_tracked_worktree(&root, &entries, &mut completeness)?;
    completeness.tracked_worktree = true;

    let untracked_raw = git_output(
        &git,
        &root,
        &["ls-files", "--others", "--exclude-standard", "-z"],
    )?;
    let untracked_paths = parse_nul_paths(&untracked_raw)?;
    if untracked_paths.len() > MAX_UNTRACKED_FILES {
        completeness.fail("untracked_file_limit");
    }
    let (untracked_digest, untracked_bytes) =
        hash_path_manifest(&root, &untracked_paths, &mut completeness)?;
    completeness.untracked = true;
    let ignored_raw = git_output(
        &git,
        &root,
        &[
            "ls-files",
            "--others",
            "--ignored",
            "--exclude-standard",
            "-z",
        ],
    )?;
    let ignored = parse_nul_paths(&ignored_raw)?;
    completeness.ignored_inputs_excluded = ignored.is_empty();
    if !ignored.is_empty() {
        completeness.fail("ignored_inputs_present");
    }

    let (toolchain_digest, executable_complete) = hash_toolchain(profile, &root)?;
    completeness.executable = executable_complete;
    completeness.toolchain = executable_complete;
    if !executable_complete {
        completeness.fail("toolchain_incomplete");
    }
    let lock_config_digest = hash_lock_config(&root)?;
    completeness.lock_config = true;
    let environment_digest = hash_environment(profile, keyed_hash_secret)?;
    completeness.environment = profile.recognized;
    let profile_digest = Some(hash_fields(&[
        &profile.name,
        &profile.allowed_env.join(","),
        "m5r1-profile-v1",
    ]));
    let operation_state_digest = Some(hash_operation_state(&git_dir)?);
    let repository_id = Some(path_identity(&common_dir)?);
    let common_dir_id = Some(path_identity(&common_dir)?);
    let worktree_id = Some(path_identity(&root)?);
    let hashed_bytes = tracked_bytes.saturating_add(untracked_bytes);
    if hashed_bytes > MAX_HASH_BYTES {
        completeness.fail("hash_byte_limit");
    }

    Ok(StateFingerprint {
        repository_id,
        common_dir_id,
        worktree_id,
        platform_id: platform_id(),
        head_ref_digest,
        detached,
        operation_state_digest,
        head_commit,
        head_tree,
        index_digest: Some(index_digest),
        staged_digest: Some(staged_digest),
        tracked_worktree_digest: Some(tracked_worktree_digest),
        untracked_digest: Some(untracked_digest),
        toolchain_digest,
        lock_config_digest: Some(lock_config_digest),
        environment_digest: Some(environment_digest),
        profile_digest,
        tracked_files: entries
            .iter()
            .map(|entry| entry.path.as_str())
            .collect::<BTreeSet<_>>()
            .len() as u64,
        untracked_files: untracked_paths.len() as u64,
        hashed_bytes,
        completeness,
    })
}

fn empty_fingerprint(completeness: Completeness) -> StateFingerprint {
    StateFingerprint {
        repository_id: None,
        common_dir_id: None,
        worktree_id: None,
        platform_id: platform_id(),
        head_ref_digest: None,
        detached: false,
        operation_state_digest: None,
        head_commit: None,
        head_tree: None,
        index_digest: None,
        staged_digest: None,
        tracked_worktree_digest: None,
        untracked_digest: None,
        toolchain_digest: None,
        lock_config_digest: None,
        environment_digest: None,
        profile_digest: None,
        tracked_files: 0,
        untracked_files: 0,
        hashed_bytes: 0,
        completeness,
    }
}

#[derive(Debug)]
struct IndexEntry {
    mode: String,
    oid: String,
    stage: String,
    path: String,
}

fn parse_index_entries(bytes: &[u8]) -> Result<Vec<IndexEntry>> {
    let mut entries = Vec::new();
    for record in bytes
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        let text = std::str::from_utf8(record).map_err(|_| {
            CoreError::Integrity("non-UTF8 Git index path is unsupported".to_string())
        })?;
        let (meta, path) = text.split_once('\t').ok_or_else(|| {
            CoreError::Integrity("malformed git ls-files --stage record".to_string())
        })?;
        validate_relative_path(Path::new(path))?;
        let mut fields = meta.split_whitespace();
        let mode = fields.next().unwrap_or("").to_string();
        let oid = fields.next().unwrap_or("").to_string();
        let stage = fields.next().unwrap_or("").to_string();
        if mode.is_empty() || oid.len() < 40 || stage.is_empty() {
            return Err(CoreError::Integrity(
                "malformed git index metadata".to_string(),
            ));
        }
        entries.push(IndexEntry {
            mode,
            oid,
            stage,
            path: path.to_string(),
        });
    }
    entries.sort_by(|left, right| {
        (&left.path, &left.stage, &left.mode, &left.oid).cmp(&(
            &right.path,
            &right.stage,
            &right.mode,
            &right.oid,
        ))
    });
    Ok(entries)
}

fn parse_nul_paths(bytes: &[u8]) -> Result<Vec<String>> {
    let mut paths = Vec::new();
    for raw in bytes.split(|byte| *byte == 0).filter(|raw| !raw.is_empty()) {
        let path = std::str::from_utf8(raw)
            .map_err(|_| CoreError::Integrity("non-UTF8 path is unsupported".to_string()))?;
        validate_relative_path(Path::new(path))?;
        paths.push(path.to_string());
    }
    paths.sort();
    Ok(paths)
}

fn hash_tracked_worktree(
    root: &Path,
    entries: &[IndexEntry],
    completeness: &mut Completeness,
) -> Result<(String, u64)> {
    let mut paths = BTreeSet::new();
    for entry in entries {
        paths.insert(entry.path.clone());
    }
    let ordered: Vec<String> = paths.into_iter().collect();
    hash_path_manifest(root, &ordered, completeness)
}

fn hash_path_manifest(
    root: &Path,
    paths: &[String],
    completeness: &mut Completeness,
) -> Result<(String, u64)> {
    let mut aggregate = Sha256::new();
    let mut bytes_hashed = 0u64;
    for relative in paths {
        let relative_path = Path::new(relative);
        validate_relative_path(relative_path)?;
        let path = root.join(relative_path);
        update_len(&mut aggregate, relative.as_bytes());
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                let target = fs::read_link(&path).map_err(|e| CoreError::io(&path, e))?;
                if target.is_absolute()
                    || target
                        .components()
                        .any(|component| component == Component::ParentDir)
                {
                    completeness.fail("escaping_symlink");
                }
                update_len(&mut aggregate, b"symlink");
                update_len(
                    &mut aggregate,
                    target.as_os_str().to_string_lossy().as_bytes(),
                );
            }
            Ok(metadata) if metadata.is_file() => {
                update_len(&mut aggregate, b"file");
                let (digest, size) = hash_file(&path)?;
                update_len(&mut aggregate, digest.as_bytes());
                bytes_hashed = bytes_hashed.saturating_add(size);
            }
            Ok(_) => {
                update_len(&mut aggregate, b"unsupported_file_type");
                completeness.fail("unsupported_file_type");
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                update_len(&mut aggregate, b"missing");
            }
            Err(error) => return Err(CoreError::io(&path, error)),
        }
    }
    Ok((to_hex(&aggregate.finalize()), bytes_hashed))
}

fn hash_lock_config(root: &Path) -> Result<String> {
    let names = [
        "Cargo.lock",
        "Cargo.toml",
        "rust-toolchain",
        "rust-toolchain.toml",
        "package.json",
        "package-lock.json",
        "pnpm-lock.yaml",
        "yarn.lock",
        ".npmrc",
        ".cargo/config.toml",
    ];
    let mut hash = Sha256::new();
    for name in names {
        let path = root.join(name);
        update_len(&mut hash, name.as_bytes());
        if path.is_file() {
            let (digest, _) = hash_file(&path)?;
            update_len(&mut hash, digest.as_bytes());
        } else {
            update_len(&mut hash, b"absent");
        }
    }
    Ok(to_hex(&hash.finalize()))
}

fn hash_environment(profile: &CommandProfile, secret: &[u8; 32]) -> Result<String> {
    let forbidden_fragments = ["SECRET", "TOKEN", "PASSWORD", "PASSWD", "PRIVATE", "KEY"];
    let mut hash = Sha256::new();
    update_len(&mut hash, secret);
    for name in &profile.allowed_env {
        if forbidden_fragments
            .iter()
            .any(|fragment| name.contains(fragment))
        {
            return Err(CoreError::Config(
                "tool profile attempted to admit a secret-shaped environment name".to_string(),
            ));
        }
        update_len(&mut hash, name.as_bytes());
        match std::env::var_os(name) {
            None => update_len(&mut hash, b"unset"),
            Some(value) if value.is_empty() => update_len(&mut hash, b"empty"),
            Some(value) => {
                update_len(&mut hash, b"present");
                update_len(&mut hash, value.to_string_lossy().as_bytes());
            }
        }
    }
    Ok(to_hex(&hash.finalize()))
}

fn hash_toolchain(profile: &CommandProfile, root: &Path) -> Result<(Option<String>, bool)> {
    let Some(executable) = &profile.executable else {
        return Ok((None, false));
    };
    if executable.starts_with(root) || !executable.is_file() {
        return Ok((None, false));
    }
    let (binary_digest, _) = hash_file(executable)?;
    let output = Command::new(executable)
        .arg("--version")
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| CoreError::io(executable, e))?;
    if !output.status.success() && output.stdout.is_empty() && output.stderr.is_empty() {
        return Ok((None, false));
    }
    let version_digest = sha256_hex(&[output.stdout, output.stderr].concat());
    Ok((
        Some(hash_fields(&[
            &binary_digest,
            &version_digest,
            &profile.name,
        ])),
        true,
    ))
}

fn hash_operation_state(git_dir: &Path) -> Result<String> {
    let names = [
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "BISECT_LOG",
        "rebase-apply/head-name",
        "rebase-merge/head-name",
    ];
    let mut hash = Sha256::new();
    for name in names {
        update_len(&mut hash, name.as_bytes());
        let path = git_dir.join(name);
        if path.is_file() {
            let (digest, _) = hash_file(&path)?;
            update_len(&mut hash, digest.as_bytes());
        } else {
            update_len(&mut hash, b"absent");
        }
    }
    Ok(to_hex(&hash.finalize()))
}

fn contains_nested_git(root: &Path) -> Result<bool> {
    let mut stack = vec![root.to_path_buf()];
    let mut visited = 0usize;
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).map_err(|e| CoreError::io(&dir, e))? {
            let entry = entry.map_err(|e| CoreError::io(&dir, e))?;
            let path = entry.path();
            let name = entry.file_name();
            if dir == root && name == OsStr::new(".git") {
                continue;
            }
            if name == OsStr::new(".git") {
                return Ok(true);
            }
            let ty = entry.file_type().map_err(|e| CoreError::io(&path, e))?;
            if ty.is_dir() && !ty.is_symlink() {
                visited += 1;
                if visited > 100_000 {
                    return Ok(true);
                }
                stack.push(path);
            }
        }
    }
    Ok(false)
}

pub fn compare_states(execution: &StateFingerprint, observation: &StateFingerprint) -> Comparison {
    if !execution.completeness.complete() || !observation.completeness.complete() {
        return Comparison {
            currentness: ProofApplicability::Unknown,
            comparable_dimensions: Vec::new(),
            changed_dimensions: Vec::new(),
            reason: "capture incomplete".to_string(),
        };
    }
    if execution.repository_id != observation.repository_id
        || execution.common_dir_id != observation.common_dir_id
        || execution.worktree_id != observation.worktree_id
        || execution.platform_id != observation.platform_id
    {
        return Comparison {
            currentness: ProofApplicability::Unknown,
            comparable_dimensions: Vec::new(),
            changed_dimensions: Vec::new(),
            reason: "repository lineage is not comparable".to_string(),
        };
    }
    let dimensions = [
        (
            "head_ref",
            &execution.head_ref_digest,
            &observation.head_ref_digest,
        ),
        (
            "head_commit",
            &execution.head_commit,
            &observation.head_commit,
        ),
        ("head_tree", &execution.head_tree, &observation.head_tree),
        (
            "operation_state",
            &execution.operation_state_digest,
            &observation.operation_state_digest,
        ),
        ("index", &execution.index_digest, &observation.index_digest),
        (
            "staged",
            &execution.staged_digest,
            &observation.staged_digest,
        ),
        (
            "tracked_worktree",
            &execution.tracked_worktree_digest,
            &observation.tracked_worktree_digest,
        ),
        (
            "untracked",
            &execution.untracked_digest,
            &observation.untracked_digest,
        ),
        (
            "toolchain",
            &execution.toolchain_digest,
            &observation.toolchain_digest,
        ),
        (
            "lock_config",
            &execution.lock_config_digest,
            &observation.lock_config_digest,
        ),
        (
            "environment",
            &execution.environment_digest,
            &observation.environment_digest,
        ),
        (
            "profile",
            &execution.profile_digest,
            &observation.profile_digest,
        ),
    ];
    let mut comparable = Vec::new();
    let mut changed = Vec::new();
    for (name, left, right) in dimensions {
        if left.is_none() || right.is_none() {
            return Comparison {
                currentness: ProofApplicability::Unknown,
                comparable_dimensions: comparable,
                changed_dimensions: changed,
                reason: format!("mandatory dimension {name} is absent"),
            };
        }
        comparable.push(name.to_string());
        if left != right {
            changed.push(name.to_string());
        }
    }
    let currentness = if changed.is_empty() {
        ProofApplicability::Current
    } else {
        ProofApplicability::Stale
    };
    Comparison {
        currentness,
        comparable_dimensions: comparable,
        changed_dimensions: changed.clone(),
        reason: if changed.is_empty() {
            "all mandatory dimensions match".to_string()
        } else {
            format!("changed dimensions: {}", changed.join(", "))
        },
    }
}

pub fn canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let value = serde_json::to_value(value)?;
    canonical_value(&value)
}

/// Verify an append-only operational record against the collector key stored
/// in the ledger. Product activation records use the same canonical encoding
/// and Ed25519 authority as capture capsules.
pub fn verify_signed_record<T: Serialize>(
    value: &T,
    expected_sha256: &str,
    signature_hex: &str,
    public_key_hex: &str,
) -> Result<bool> {
    let bytes = canonical_json(value)?;
    if sha256_hex(&bytes) != expected_sha256 {
        return Ok(false);
    }
    let public = decode_hex_fixed::<32>(public_key_hex)?;
    let verifying = VerifyingKey::from_bytes(&public)
        .map_err(|_| CoreError::Integrity("invalid operational public key".to_string()))?;
    let signature = Signature::from_bytes(&decode_hex_fixed::<64>(signature_hex)?);
    Ok(verifying.verify(&bytes, &signature).is_ok())
}

fn canonical_value(value: &Value) -> Result<Vec<u8>> {
    let normalized = sort_json(value);
    serde_json::to_vec(&normalized).map_err(Into::into)
}

fn sort_json(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut out = Map::new();
            for key in keys {
                if let Some(value) = map.get(key) {
                    out.insert(key.clone(), sort_json(value));
                }
            }
            Value::Object(out)
        }
        Value::Array(values) => Value::Array(values.iter().map(sort_json).collect()),
        _ => value.clone(),
    }
}

pub fn attempt_id(request: &CaptureRequest, command_sha256: &str) -> String {
    let fields = [
        request.owner_key.as_bytes(),
        request.source.as_bytes(),
        request.source_project_key.as_bytes(),
        request.source_session_key.as_bytes(),
        request.source_turn_id.as_deref().unwrap_or("").as_bytes(),
        request.source_tool_id.as_bytes(),
        request.wrapper_nonce.as_bytes(),
        command_sha256.as_bytes(),
        request.source_config_sha256.as_bytes(),
        b"m5r1-attempt-v1",
    ];
    let mut hash = Sha256::new();
    for field in fields {
        update_len(&mut hash, field);
    }
    format!("cap_{}", to_hex(&hash.finalize()))
}

/// Privacy-safe capture identity for a source-native project key. The raw key
/// (often an absolute root in Codex) stays in the pre-existing source ledger
/// and the private invocation, never in M5 capture rows or public exports.
pub fn opaque_source_project_key(owner: &str, source: &str, native_key: &str) -> String {
    format!(
        "cap_project_{}",
        hash_fields(&[owner, source, native_key, "m5r1-source-project-v1"])
    )
}

fn capture_strength(request: &CaptureRequest, state: &StateFingerprint) -> CaptureStrength {
    if request.assurance_enabled && request.source == "codex" && state.completeness.complete() {
        CaptureStrength::Assured
    } else if state.repository_id.is_some() {
        CaptureStrength::BoundedObservation
    } else {
        CaptureStrength::Unknown
    }
}

fn execution_strength(
    request: &CaptureRequest,
    pre: Option<&SignedCapsule>,
    post: Option<&SignedCapsule>,
) -> CaptureStrength {
    let (Some(pre), Some(post)) = (pre, post) else {
        return CaptureStrength::Unknown;
    };
    if pre.payload.state != post.payload.state {
        return CaptureStrength::Unknown;
    }
    if request.assurance_enabled
        && request.source == "codex"
        && pre.payload.state.completeness.complete()
        && post.payload.state.completeness.complete()
    {
        CaptureStrength::Assured
    } else if pre.payload.state.repository_id.is_some() {
        CaptureStrength::BoundedObservation
    } else {
        CaptureStrength::Unknown
    }
}

fn signature_binding(
    capsule_id: &str,
    attempt_id: &str,
    phase: CapsulePhase,
    prior: Option<&str>,
    complete: bool,
) -> Vec<u8> {
    let mut bytes = Vec::new();
    for field in [
        b"thruline-capsule-signature-v1".as_slice(),
        capsule_id.as_bytes(),
        attempt_id.as_bytes(),
        phase.as_str().as_bytes(),
        prior.unwrap_or("").as_bytes(),
        if complete { b"complete" } else { b"incomplete" },
    ] {
        bytes.extend_from_slice(&(field.len() as u64).to_le_bytes());
        bytes.extend_from_slice(field);
    }
    bytes
}

/// Run a captured command.
///
/// # `/bin/sh -c`, and why neither half is incidental
///
/// This ran `/bin/zsh -lc`, which is macOS's default login shell and is absent
/// from a stock Linux image. Track C measured the consequence in
/// `debian:bookworm-slim`, with ThruLine installed from the published
/// `curl | sh` path:
///
/// ```text
/// $ thruline demo --continuity
/// error: running the demo: i/o error at /bin/zsh: No such file or directory
/// ```
///
/// §15 makes `thruline demo` the first thing a new user is told to run, so the
/// first command the installer recommends did not run on Linux at all. It
/// survived because it is unreachable from a developer's Mac: only
/// `packaging/acceptance-fresh-machine.sh` runs the install path on a machine
/// that has never seen ThruLine.
///
/// **`$SHELL` is not consulted.** What a command *means* has to be the same on
/// the machine that captured it and the machine that verifies it; a shell taken
/// from the environment makes the recorded `shell_profile` a property of
/// whoever ran it rather than of the run.
///
/// **`-c`, not `-lc`.** A login shell sources dotfiles that are not in the
/// record and cannot be re-executed with it — a receipt over a command whose
/// meaning came from an unrecorded file is the failure a receipt exists to
/// prevent, one layer down.
///
/// This also removes the stated reason CI's `test` job is macOS-only
/// (`docs/P0-BUILD-BRIEF.md:1649`: *"`capture.rs` ... hardcodes
/// `Command::new("/bin/zsh")` with no fallback"*). Widening the job is a
/// separate change and is not claimed here.
fn execute_shell(command: &str, cwd: &Path) -> Result<ExitStatus> {
    Command::new("/bin/sh")
        .arg("-c")
        .arg(command)
        .current_dir(cwd)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| CoreError::io("/bin/sh", e))
}

fn exit_parts(status: &ExitStatus) -> (Option<i32>, Option<i32>) {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        (status.code(), status.signal())
    }
    #[cfg(not(unix))]
    {
        (status.code(), None)
    }
}

pub(crate) fn trusted_git() -> Result<PathBuf> {
    let system = PathBuf::from("/usr/bin/git");
    if system.is_file() {
        return Ok(system);
    }
    resolve_executable("git")?
        .ok_or_else(|| CoreError::Config("git executable not found".to_string()))
}

pub(crate) fn git_output(git: &Path, cwd: &Path, args: &[&str]) -> Result<Vec<u8>> {
    // **`/dev/null`, not a file in the shared temp directory.**
    //
    // This used to point `GIT_CONFIG_GLOBAL` at a FIXED-NAME path,
    // `$TMPDIR/thruline-empty-git-config`, created only `if !exists()` with the
    // result discarded. Two consequences, and the second is the serious one:
    // the file was never removed, so every run left one behind; and a file
    // ALREADY at that path was not rejected but silently obeyed as the global
    // git config for every plumbing call below.
    //
    // `git_output` is what `workspace_identity`'s `ls-files -s` and
    // `protected_store`'s `cat-file blob` go through — the blob hashes that
    // decide which tree a receipt is about. `GIT_CONFIG_NOSYSTEM=1` had already
    // turned off the system config that might have conflicted, and only
    // `core.fsmonitor` and `core.hooksPath` are pinned with `-c`; anything else
    // in that file was honoured. Demonstrated by the guard in this module,
    // which plants `status.showUntrackedFiles=no` and watched an untracked file
    // disappear from `status --porcelain`.
    //
    // An empty config needs no file. `/dev/null` is empty, is not writable by
    // another user, and cannot be pre-created — so there is nothing to leak and
    // nothing to plant. A per-process name would have fixed the injection and
    // kept the leak.
    git_output_within(git, cwd, args, crate::bounded_child::TIME_LIMIT)
}

/// [`git_output`] with its time limit named (SNAPSHOT-CHILD-TIMEOUT, D-051).
///
/// The child runs in its own process group. If it has not finished by `limit`,
/// the group is killed, the child is reaped, and the answer is an error of kind
/// `TimedOut` ([`crate::bounded_child::is_timeout`]) with no output. A partial
/// `ls-files` or `cat-file` would hash a tree nobody has, so nothing is kept.
/// stdin is null, as before. stderr is now captured and discarded where it used
/// to go to `/dev/null`, which is the same from git's side: neither is a
/// terminal.
pub(crate) fn git_output_within(
    git: &Path,
    cwd: &Path,
    args: &[&str],
    limit: std::time::Duration,
) -> Result<Vec<u8>> {
    let mut command = Command::new(git);
    command
        .args([
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .current_dir(cwd)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("LC_ALL", "C");
    let output =
        match crate::bounded_child::output_until(&mut command, std::time::Instant::now() + limit)
            .map_err(|e| CoreError::io(git, e))?
        {
            crate::bounded_child::Outcome::Finished(output) => output,
            crate::bounded_child::Outcome::TimedOut => {
                return Err(crate::bounded_child::timed_out(
                    &format!("git {}", args.join(" ")),
                    limit,
                ))
            }
        };
    if !output.status.success() {
        return Err(CoreError::Integrity(format!(
            "trusted git plumbing failed: {}",
            args.join(" ")
        )));
    }
    Ok(output.stdout)
}

/// `None` means git answered and said no. A read that did not finish is not
/// that answer — a timed-out `rev-parse --show-toplevel` is not "not a
/// worktree" — so a timeout is passed on as the error it is.
pub(crate) fn git_optional(git: &Path, cwd: &Path, args: &[&str]) -> Result<Option<String>> {
    optional(git_output(git, cwd, args))
}

/// The worktree root from `rev-parse --show-toplevel`, or `None` when git
/// answered that `cwd` is not in a worktree. A read that timed out is not that
/// answer (SNAPSHOT-CHILD-TIMEOUT, D-051): it is passed on as an error, so the
/// capture fails instead of recording `non_git_directory` for a folder git
/// never described.
fn repository_root(read: Result<Vec<u8>>) -> Result<Option<PathBuf>> {
    match read {
        Ok(bytes) => Ok(Some(PathBuf::from(trim_output(&bytes)?))),
        Err(err) if crate::bounded_child::is_timeout(&err) => Err(err),
        Err(_) => Ok(None),
    }
}

fn optional(read: Result<Vec<u8>>) -> Result<Option<String>> {
    match read {
        Ok(bytes) => Ok(Some(trim_output(&bytes)?)),
        Err(err) if crate::bounded_child::is_timeout(&err) => Err(err),
        Err(_) => Ok(None),
    }
}

fn resolve_git_path(root: &Path, bytes: &[u8]) -> Result<PathBuf> {
    let raw = PathBuf::from(trim_output(bytes)?);
    let path = if raw.is_absolute() {
        raw
    } else {
        root.join(raw)
    };
    fs::canonicalize(&path).map_err(|e| CoreError::io(&path, e))
}

fn trim_output(bytes: &[u8]) -> Result<String> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| CoreError::Integrity("non-UTF8 tool output".to_string()))?;
    Ok(text.trim().to_string())
}

fn resolve_executable(token: &str) -> Result<Option<PathBuf>> {
    let candidate = PathBuf::from(token);
    if candidate.components().count() > 1 {
        if candidate.is_file() {
            return fs::canonicalize(&candidate)
                .map(Some)
                .map_err(|e| CoreError::io(&candidate, e));
        }
        return Ok(None);
    }
    let Some(path) = std::env::var_os("PATH") else {
        return Ok(None);
    };
    for directory in std::env::split_paths(&path) {
        let full = directory.join(token);
        if full.is_file() {
            return fs::canonicalize(&full)
                .map(Some)
                .map_err(|e| CoreError::io(&full, e));
        }
    }
    Ok(None)
}

fn path_identity(path: &Path) -> Result<String> {
    let canonical = fs::canonicalize(path).map_err(|e| CoreError::io(path, e))?;
    let metadata = fs::metadata(&canonical).map_err(|e| CoreError::io(&canonical, e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(hash_fields(&[
            &canonical.to_string_lossy(),
            &metadata.dev().to_string(),
            &metadata.ino().to_string(),
            &platform_id(),
        ]))
    }
    #[cfg(not(unix))]
    {
        Ok(hash_fields(&[
            &canonical.to_string_lossy(),
            &metadata.len().to_string(),
            &platform_id(),
        ]))
    }
}

fn platform_id() -> String {
    format!("{}:{}", std::env::consts::OS, std::env::consts::ARCH)
}

fn validate_relative_path(path: &Path) -> Result<()> {
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(CoreError::Integrity(
            "capture manifest path escapes repository".to_string(),
        ));
    }
    Ok(())
}

fn hash_file(path: &Path) -> Result<(String, u64)> {
    let mut file = File::open(path).map_err(|e| CoreError::io(path, e))?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let read = file.read(&mut buffer).map_err(|e| CoreError::io(path, e))?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
        total = total.saturating_add(read as u64);
    }
    Ok((to_hex(&hash.finalize()), total))
}

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let metadata = fs::metadata(path).map_err(|e| CoreError::io(path, e))?;
    if metadata.len() > limit {
        return Err(CoreError::Integrity(format!(
            "private capture record exceeds {limit} bytes"
        )));
    }
    fs::read(path).map_err(|e| CoreError::io(path, e))
}

fn hash_fields(fields: &[&str]) -> String {
    let mut hash = Sha256::new();
    for field in fields {
        update_len(&mut hash, field.as_bytes());
    }
    to_hex(&hash.finalize())
}

fn update_len(hash: &mut Sha256, bytes: &[u8]) {
    hash.update((bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
}

fn duration_ns(duration: Duration) -> u64 {
    duration.as_nanos().min(u64::MAX as u128) as u64
}

fn unix_millis() -> Result<u64> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| CoreError::Integrity("system clock precedes Unix epoch".to_string()))?
        .as_millis();
    u64::try_from(millis)
        .map_err(|_| CoreError::Integrity("capture timestamp exceeds u64".to_string()))
}

fn secure_dir(path: &Path) -> Result<()> {
    if !path.exists() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(path)
                .map_err(|e| CoreError::io(path, e))?;
        }
        #[cfg(not(unix))]
        fs::create_dir_all(path).map_err(|e| CoreError::io(path, e))?;
    }
    let metadata = fs::symlink_metadata(path).map_err(|e| CoreError::io(path, e))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(CoreError::Integrity(
            "capture root component is not a real directory".to_string(),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if metadata.uid() != unsafe_free_effective_uid()? {
            return Err(CoreError::Integrity(
                "capture directory is owned by another user".to_string(),
            ));
        }
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|e| CoreError::io(path, e))?;
    }
    Ok(())
}

#[cfg(unix)]
fn unsafe_free_effective_uid() -> Result<u32> {
    // Avoid libc/unsafe: a freshly created file is an authoritative local uid
    // witness, and MetadataExt exposes its owner safely.
    use std::os::unix::fs::MetadataExt;
    let probe = std::env::temp_dir().join(format!("thruline-uid-{}", std::process::id()));
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&probe)
        .map_err(|e| CoreError::io(&probe, e))?;
    let uid = file.metadata().map_err(|e| CoreError::io(&probe, e))?.uid();
    drop(file);
    let _ = fs::remove_file(&probe);
    Ok(uid)
}

fn load_or_create_key(path: &Path) -> Result<SigningKey> {
    if path.exists() {
        let bytes = read_bounded(path, 32)?;
        if bytes.len() != 32 {
            return Err(CoreError::Integrity(
                "collector key has invalid length".to_string(),
            ));
        }
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&bytes);
        return Ok(SigningKey::from_bytes(&seed));
    }
    let key = SigningKey::generate(&mut OsRng);
    write_exclusive_idempotent(path, key.as_bytes())?;
    Ok(key)
}

fn write_exclusive_idempotent(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        secure_dir(parent)?;
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(path) {
        Ok(mut file) => {
            file.write_all(bytes).map_err(|e| CoreError::io(path, e))?;
            file.sync_all().map_err(|e| CoreError::io(path, e))?;
            if let Some(parent) = path.parent() {
                File::open(parent)
                    .and_then(|directory| directory.sync_all())
                    .map_err(|e| CoreError::io(parent, e))?;
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let existing = fs::read(path).map_err(|e| CoreError::io(path, e))?;
            if existing == bytes {
                Ok(())
            } else {
                Err(CoreError::Integrity(format!(
                    "conflicting immutable capture record at {}",
                    path.display()
                )))
            }
        }
        Err(error) => Err(CoreError::io(path, error)),
    }
}

fn decode_hex_fixed<const N: usize>(text: &str) -> Result<[u8; N]> {
    if text.len() != N * 2 {
        return Err(CoreError::Integrity(
            "hex value has invalid length".to_string(),
        ));
    }
    let mut out = [0u8; N];
    for (index, pair) in text.as_bytes().chunks_exact(2).enumerate() {
        let high = hex_nibble(pair[0])?;
        let low = hex_nibble(pair[1])?;
        out[index] = (high << 4) | low;
    }
    Ok(out)
}

fn hex_nibble(byte: u8) -> Result<u8> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(CoreError::Integrity("invalid hex value".to_string())),
    }
}

/// R17a-3 — bind accepted **assured** capture pairs to the `tool_use` events
/// they attest, for one project.
///
/// The binding re-derives `exact_source_command_match`'s predicate at
/// evaluation time (A103): within one `(session_id, source_turn_id)`, exactly
/// one `tool_use` whose command hashes to the attempt's `command_sha256`.
/// **Uniqueness is required** — two candidates means we cannot say which run
/// was attested, and the honest answer is to bind neither.
///
/// The `tool_result` is never touched. A102 recorded that the pair was
/// reachable via `capture_event_links.source_tool_id == events.tool_use_id`;
/// that is false (the first stores the raw vendor call id, the second a
/// namespace hash), and the obvious repair — walking the sibling — is the join
/// ruling 2 forbids. See `the_capture_link_id_is_not_the_event_tool_use_id`.
///
/// `tool_input` is read from `events` directly. Derive's in-memory copy is a
/// 2,000-character prefix, and hashing a truncated command would either fail to
/// bind or bind the wrong run.
pub fn attested_runs_for_project(
    ledger: &Ledger,
    project_id: &str,
) -> Result<std::collections::BTreeMap<String, crate::contract::AttestedRun>> {
    // The vendor scopes every registry lookup below: a tool name means nothing
    // without the vendor that produced it.
    let vendor: String = ledger.conn().query_row(
        "SELECT source FROM projects WHERE id = ?1",
        params![project_id],
        |r| r.get(0),
    )?;
    let mut attempts = ledger.conn().prepare(
        "SELECT a.command_sha256, a.exit_code, a.strength, a.status, a.source_turn_id,
                ss.session_id,
                pre.head_commit, pre.head_tree, pre.index_digest,
                pre.tracked_worktree_digest, pre.staged_digest, pre.untracked_digest,
                post.head_commit, post.head_tree, post.index_digest,
                post.tracked_worktree_digest, post.staged_digest, post.untracked_digest
         FROM capture_attempts a
         JOIN source_sessions ss
           ON ss.source_session_key = a.source_session_key
          AND ss.owner_key = a.owner_key
          AND ss.source = a.source
         JOIN state_capsules pre ON pre.attempt_id = a.attempt_id AND pre.phase = 'pre'
         JOIN state_capsules post ON post.attempt_id = a.attempt_id AND post.phase = 'post'
         WHERE ss.project_id = ?1
           AND a.strength = 'assured'
           AND a.status = 'accepted'
         ORDER BY a.attempt_id",
    )?;
    type Row = (
        String,
        Option<i64>,
        String,
        String,
        Option<String>,
        String,
        Vec<Option<String>>,
        Vec<Option<String>>,
        String,
        String,
    );
    let rows = attempts.query_map(params![project_id], |r| {
        let pre: Vec<Option<String>> = (7..12).map(|i| r.get(i).ok().flatten()).collect();
        let post: Vec<Option<String>> = (13..18).map(|i| r.get(i).ok().flatten()).collect();
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<i64>>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, Option<String>>(4)?,
            r.get::<_, String>(5)?,
            pre,
            post,
            r.get::<_, Option<String>>(12)?.unwrap_or_default(),
            r.get::<_, Option<String>>(6)?.unwrap_or_default(),
        ))
    })?;

    let mut out = std::collections::BTreeMap::new();
    for row in rows {
        let (
            command_sha256,
            exit_code,
            strength,
            status,
            turn,
            session_id,
            pre,
            post,
            post_head,
            pre_head,
        ): Row = row?;
        let Some(turn) = turn else { continue };
        // The fingerprint must not have moved across the run. `accept_capture_pair`
        // already refuses otherwise; stated here so this path carries its own
        // precondition rather than trusting a distant caller.
        let fingerprint_stable = pre == post && pre_head == post_head;

        // Re-derive the validated bridge. Candidates are the turn's tool_uses;
        // the command comes from the FULL stored input.
        let mut candidates = ledger.conn().prepare(
            "SELECT id, COALESCE(tool_input,''), COALESCE(tool_name,''), source_tool_name
             FROM events
             WHERE session_id = ?1 AND source_turn_id = ?2 AND kind = 'tool_use'
             ORDER BY seq, id",
        )?;
        let mut matching: Vec<String> = Vec::new();
        for candidate in candidates.query_map(params![session_id, turn], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })? {
            let (event_id, input, canonical, raw) = candidate?;
            if registered_command(&vendor, raw_tool_name(&canonical, raw.as_deref()), &input)
                .map(|c| sha256_hex(c.as_bytes()) == command_sha256)
                .unwrap_or(false)
            {
                matching.push(event_id);
            }
        }
        // Uniqueness, or nothing. Two runs of the same command in one turn are
        // indistinguishable from here, and guessing which one was attested is
        // exactly how a receipt gets attached to the wrong work.
        if matching.len() != 1 {
            continue;
        }
        out.insert(
            matching.remove(0),
            crate::contract::AttestedRun {
                assured: strength == "assured",
                accepted: status == "accepted",
                command_sha256,
                exit_code,
                head_commit: post_head,
                fingerprint_stable,
            },
        );
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// SNAPSHOT-CHILD-TIMEOUT (D-051). `git_optional`'s `None` is "git said
    /// no". A read that timed out is passed on as the error it is, so that a
    /// stalled `rev-parse --show-toplevel` never reads as "not a worktree".
    #[test]
    fn a_timed_out_read_is_not_git_saying_no() {
        let timed_out = optional(Err(crate::bounded_child::timed_out(
            "git rev-parse --show-toplevel",
            crate::bounded_child::TIME_LIMIT,
        )));
        assert!(
            matches!(&timed_out, Err(e) if crate::bounded_child::is_timeout(e)),
            "{timed_out:?}"
        );
        let said_no = optional(Err(CoreError::Integrity(
            "trusted git plumbing failed: rev-parse --show-toplevel".into(),
        )));
        assert!(matches!(said_no, Ok(None)), "{said_no:?}");
        let answered = optional(Ok(b"/w/repo\n".to_vec()));
        assert_eq!(answered.expect("answer").as_deref(), Some("/w/repo"));
    }

    /// SNAPSHOT-CHILD-TIMEOUT (D-051). The execution capture records
    /// `non_git_directory` when `rev-parse --show-toplevel` says no. A read of
    /// it that timed out must fail the capture instead of becoming that value.
    #[test]
    fn a_timed_out_root_read_is_not_a_non_git_directory() {
        let timed_out = repository_root(Err(crate::bounded_child::timed_out(
            "git rev-parse --show-toplevel",
            crate::bounded_child::TIME_LIMIT,
        )));
        assert!(
            matches!(&timed_out, Err(e) if crate::bounded_child::is_timeout(e)),
            "{timed_out:?}"
        );
        let said_no = repository_root(Err(CoreError::Integrity(
            "trusted git plumbing failed: rev-parse --show-toplevel".into(),
        )));
        assert!(matches!(said_no, Ok(None)), "{said_no:?}");
        let answered = repository_root(Ok(b"/w/repo\n".to_vec())).expect("answer");
        assert_eq!(answered, Some(PathBuf::from("/w/repo")));
    }

    /// SNAPSHOT-CHILD-TIMEOUT (D-051). A git that does not answer is killed at
    /// the limit, with what it started, and the answer is a timeout — not
    /// output, and not the `None` that `git_optional` gives for "git said no".
    #[cfg(unix)]
    #[test]
    fn a_git_that_does_not_answer_is_a_timeout_not_an_answer() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempdir().expect("tempdir");
        let pids = dir.path().join("pids");
        let git = dir.path().join("git");
        std::fs::write(
            &git,
            format!(
                "#!/bin/sh\necho $$ >> '{p}'\nsleep 60 &\necho $! >> '{p}'\nwait\n",
                p = pids.display()
            ),
        )
        .expect("stand-in");
        std::fs::set_permissions(&git, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        let started = std::time::Instant::now();
        let err = git_output_within(
            &git,
            dir.path(),
            &["rev-parse", "--show-toplevel"],
            std::time::Duration::from_secs(5),
        )
        .expect_err("a stalled git answered");
        assert!(crate::bounded_child::is_timeout(&err), "{err}");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(10),
            "{:?}",
            started.elapsed()
        );
        let text = std::fs::read_to_string(&pids).expect("pids");
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while text.lines().any(|pid| {
            Command::new("kill")
                .args(["-0", pid])
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        }) {
            assert!(std::time::Instant::now() < until, "left behind: {text:?}");
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }

    #[test]
    fn canonical_json_sorts_object_keys() {
        let value = serde_json::json!({"z":1,"a":{"d":2,"b":3}});
        assert_eq!(
            String::from_utf8(canonical_value(&value).unwrap()).unwrap(),
            r#"{"a":{"b":3,"d":2},"z":1}"#
        );
    }

    #[test]
    fn collector_signatures_detect_tampering() {
        let temp = tempdir().unwrap();
        let collector = Collector::open(temp.path(), "owner", "codex").unwrap();
        let mut completeness = Completeness::new();
        completeness.git_repository = true;
        let payload = CapsulePayload {
            schema_version: 1,
            attempt_id: "attempt".to_string(),
            phase: CapsulePhase::Observation,
            prior_capsule_id: None,
            owner_key: "owner".to_string(),
            source: "codex".to_string(),
            source_project_key: "project".to_string(),
            source_session_key: "session".to_string(),
            source_turn_id: None,
            source_tool_id: "tool".to_string(),
            command_sha256: sha256_hex(b"git status"),
            source_config_sha256: sha256_hex(b"config"),
            source_version: "test".to_string(),
            profile: "git_v1".to_string(),
            strength: CaptureStrength::Unknown,
            observed_at_unix_ms: 1,
            capture_duration_ns: 1,
            state: empty_fingerprint(completeness),
        };
        let (mut signed, _) = collector.seal_capsule(payload).unwrap();
        collector.verify_capsule(&signed).unwrap();
        signed.payload.profile = "tampered".to_string();
        assert!(collector.verify_capsule(&signed).is_err());
    }

    #[test]
    fn claude_never_enables_assured_from_compatible_schema() {
        let measurement = SourceAssuranceMeasurement {
            source: "claude_code".to_string(),
            source_version: "compatible".to_string(),
            config_sha256: sha256_hex(b"config"),
            matcher: "Bash".to_string(),
            pre_blocks: true,
            rewrite_exact: true,
            stable_tool_identity: true,
            completion_pairing: true,
            measured_pre_delay_ms: 400,
            child_started_after_pre: true,
            wrapper_exit_code: 7,
            source_exit_code: 7,
        };
        assert!(!measurement.enables_assured());
    }

    /// A global git config planted in `TMPDIR` must not reach `git_output`.
    ///
    /// `git_output` used to point `GIT_CONFIG_GLOBAL` at a **fixed-name** file
    /// in the shared temp directory, `thruline-empty-git-config`, creating it
    /// only `if !exists()` and discarding the result either way. So a file
    /// already at that path was never rejected — it was silently honoured as
    /// the global git config for every plumbing call the engine makes.
    ///
    /// That path is not incidental. `git_output` is what
    /// `workspace_identity.rs:584` and `protected_store.rs:800,861,1385` use
    /// for `ls-files -s` and `cat-file blob` — the blob hashes that decide
    /// which tree a receipt is about. `GIT_CONFIG_NOSYSTEM=1` had already
    /// disabled the system config that might otherwise have conflicted, and
    /// only `core.fsmonitor` and `core.hooksPath` are pinned with `-c`;
    /// everything else in that file was obeyed.
    ///
    /// This asserts the CONSEQUENCE — the setting does not take effect, and no
    /// file is created — rather than that some call returned `Ok`. The old code
    /// discarded exactly such a result.
    ///
    /// It plants at the real `temp_dir()` because that is the path under test.
    /// That is safe for other tests **because of the fix**: with
    /// `GIT_CONFIG_GLOBAL=/dev/null` nothing reads the planted file, so it
    /// cannot perturb a concurrent `git_output`.
    #[test]
    fn a_planted_global_git_config_is_ignored_and_no_temp_file_is_created() {
        let Ok(git) = trusted_git() else {
            panic!("no trusted git on PATH; this guard did NOT run");
        };
        let scratch = tempfile::tempdir().expect("tempdir");
        let repo = scratch.path().join("repo");
        std::fs::create_dir_all(&repo).expect("mkdir");
        let run = |args: &[&str]| {
            let ok = std::process::Command::new(&git)
                .args(args)
                .current_dir(&repo)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .expect("git")
                .success();
            assert!(ok, "git {args:?} failed");
        };
        run(&["init", "-q", "."]);
        std::fs::write(repo.join("untracked.txt"), "x").expect("write");

        let planted = std::env::temp_dir().join("thruline-empty-git-config");

        // Restoration lives in a `Drop`, not after the assertions. E measured
        // the alternative tonight: a trailing cleanup is skipped on exactly the
        // runs a suite exists to have (class_two_corpora.rs:820, whose cleanup
        // sat after an `assert_eq!` and so leaked only when the assert failed).
        struct Restore {
            path: PathBuf,
            was: Option<Vec<u8>>,
        }
        impl Drop for Restore {
            fn drop(&mut self) {
                match &self.was {
                    Some(bytes) => {
                        let _ = std::fs::write(&self.path, bytes);
                    }
                    None => {
                        let _ = std::fs::remove_file(&self.path);
                    }
                }
            }
        }
        let _restore = Restore {
            path: planted.clone(),
            was: std::fs::read(&planted).ok(),
        };

        // --- the injection half -------------------------------------------
        std::fs::write(&planted, "[status]\n\tshowUntrackedFiles = no\n").expect("plant");
        let bytes = git_output(&git, &repo, &["status", "--porcelain"]).expect("status");
        let text = String::from_utf8_lossy(&bytes);
        assert!(
            text.contains("untracked.txt"),
            "a git config planted at {} was HONOURED: `status --porcelain` hid \
             an untracked file because that file set \
             status.showUntrackedFiles=no.\n\n\
             SCOPE: `git_output` pointed GIT_CONFIG_GLOBAL at a fixed-name path \
             in the shared temp dir, so any user able to create that path \
             controlled the global git config for every plumbing call the \
             engine makes — including the `ls-files -s` and `cat-file blob` \
             that establish a receipt's subject tree. The fix is \
             GIT_CONFIG_GLOBAL=/dev/null: no temp path, nothing to plant.\n\n\
             got: {text:?}",
            planted.display()
        );

        // --- the leak half -------------------------------------------------
        // Checked from a KNOWN-ABSENT state. The first version of this test
        // restored a pre-existing file and then asserted it was absent, which
        // is unfalsifiable in one direction and failed for the wrong reason.
        std::fs::remove_file(&planted).expect("clear before the leak check");
        let _ = git_output(&git, &repo, &["status", "--porcelain"]).expect("status again");
        assert!(
            !planted.exists(),
            "git_output created {} — it must not write into the shared temp \
             directory at all. A per-run name would have closed the injection \
             and kept this leak; /dev/null closes both.",
            planted.display()
        );
    }
}
