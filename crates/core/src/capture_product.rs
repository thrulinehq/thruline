//! M5.R2 product activation for Assured Capture.
//!
//! This module owns operational eligibility only. It cannot create a truth
//! verdict, rewrite an event, or mutate an accepted capture. Every activating
//! decision is append-only and scoped by owner, source, exact executable,
//! exact effective configuration, wrapper, collector, self-test, and project.

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::capture::{canonical_json, verify_signed_record, Collector, CAPSULE_SCHEMA_VERSION};
use crate::chain::sha256_hex;
use crate::error::{CoreError, Result};
use crate::ledger::Ledger;

pub const M5R2_PROFILE_SCHEMA_VERSION: u32 = 1;
pub const QUALIFIED_CODEX_VERSION: &str = "0.146.0-alpha.9.2";
pub const M5R1_MEASURED_CONFIG_SHA256: &str =
    "43caf02c21c23bb58196647a11bfa71fe7afb694599753e7f1068f9fb99a37dd";
pub const M5R1_BOUNDARY_REFERENCE: &str = "A76/M5.R1 exact native boundary";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallPlan {
    pub schema_version: u32,
    pub plan_id: String,
    pub owner_key: String,
    pub source: String,
    pub config_identity: String,
    pub config_preimage_sha256: String,
    pub proposed_config_sha256: String,
    pub source_version: String,
    pub source_executable_sha256: String,
    pub wrapper_sha256: String,
    pub matcher: Vec<String>,
    pub proposed_config: serde_json::Value,
    pub created_at_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallPlanInput {
    pub owner_key: String,
    pub source: String,
    pub config_identity: String,
    pub config_preimage_sha256: String,
    pub proposed_config_sha256: String,
    pub source_version: String,
    pub source_executable_sha256: String,
    pub wrapper_sha256: String,
    pub matcher: Vec<String>,
    pub proposed_config: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecyclePayload {
    pub schema_version: u32,
    pub owner_key: String,
    pub source: String,
    pub action: String,
    pub supersedes_record_id: Option<String>,
    pub plan_id: Option<String>,
    pub config_identity: String,
    pub config_sha256: Option<String>,
    pub source_version: Option<String>,
    pub source_executable_sha256: Option<String>,
    pub wrapper_sha256: Option<String>,
    pub collector_key_id: Option<String>,
    pub detail: serde_json::Value,
    pub accepted_at_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelfTestPayload {
    pub schema_version: u32,
    pub owner_key: String,
    pub source: String,
    pub source_version: String,
    pub source_executable_sha256: String,
    pub config_sha256: String,
    pub wrapper_sha256: String,
    pub collector_key_id: String,
    pub native_boundary: bool,
    pub exit_zero_preserved: bool,
    pub exit_seven_preserved: bool,
    pub completion_pairing: bool,
    pub cleanup_complete: bool,
    pub detail: serde_json::Value,
    pub accepted_at_unix_ms: u64,
}

impl SelfTestPayload {
    pub fn passed(&self) -> bool {
        self.source == "codex"
            && self.source_version == QUALIFIED_CODEX_VERSION
            && self.native_boundary
            && self.exit_zero_preserved
            && self.exit_seven_preserved
            && self.completion_pairing
            && self.cleanup_complete
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompatibilityProfilePayload {
    pub schema_version: u32,
    pub owner_key: String,
    pub source: String,
    pub source_version: String,
    pub source_executable_sha256: String,
    pub config_sha256: String,
    pub matcher: Vec<String>,
    pub wrapper_sha256: String,
    pub capsule_schema_version: u32,
    pub collector_key_id: String,
    pub permissions_valid: bool,
    pub self_test_id: String,
    pub measured_boundary_ref: String,
    pub supersedes_profile_id: Option<String>,
    pub accepted_at_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorizationPayload {
    pub schema_version: u32,
    pub owner_key: String,
    pub source: String,
    pub source_project_key: String,
    pub action: String,
    pub reason: String,
    pub accepted_at_unix_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureReadiness {
    NotAvailable,
    BoundedOnly,
    Disabled,
    AvailableNotInstalled,
    DegradedSourceVersion,
    DegradedConfigurationDrift,
    DegradedKeyOrIdentity,
    InstalledUnverified,
    DegradedSourcePairing,
    UnsupportedProfile,
    CaptureError,
    AssuredReady,
}

impl CaptureReadiness {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotAvailable => "not_available",
            Self::BoundedOnly => "bounded_only",
            Self::Disabled => "disabled",
            Self::AvailableNotInstalled => "available_not_installed",
            Self::DegradedSourceVersion => "degraded_source_version",
            Self::DegradedConfigurationDrift => "degraded_configuration_drift",
            Self::DegradedKeyOrIdentity => "degraded_key_or_identity",
            Self::InstalledUnverified => "installed_unverified",
            Self::DegradedSourcePairing => "degraded_source_pairing",
            Self::UnsupportedProfile => "unsupported_profile",
            Self::CaptureError => "capture_error",
            Self::AssuredReady => "assured_ready",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureReadinessInput {
    pub owner_key: String,
    pub source: String,
    pub source_project_key: Option<String>,
    pub source_present: bool,
    pub source_version: Option<String>,
    pub source_executable_sha256: Option<String>,
    pub config_identity: String,
    pub config_sha256: Option<String>,
    pub wrapper_sha256: Option<String>,
    pub collector_key_id: Option<String>,
    pub collector_permissions_valid: bool,
    pub matcher_valid: bool,
    pub duplicate_owned_entries: bool,
    pub source_pairing_complete: bool,
    pub selected_command_profile_supported: bool,
    pub latest_capture_error: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureReadinessReport {
    pub state: CaptureReadiness,
    pub reason: String,
    pub owner_key: String,
    pub source: String,
    pub source_project_key: Option<String>,
    pub profile_id: Option<String>,
    pub latest_lifecycle_action: Option<String>,
    pub accepted_capture_count: u64,
    pub incomplete_capture_count: u64,
    pub latest_capture_at_unix_ms: Option<u64>,
    pub source_available: bool,
    pub matcher_valid: bool,
    pub duplicate_owned_entries: bool,
    pub collector_key_available: bool,
    pub collector_permissions_valid: bool,
    pub source_pairing_complete: bool,
    pub selected_command_profile_supported: bool,
    pub project_authorized: bool,
    pub proof_currency: BTreeMap<String, u64>,
    pub historical_evidence_retained: bool,
}

pub struct NewSelfTestInput {
    pub source_version: String,
    pub source_executable_sha256: String,
    pub config_sha256: String,
    pub wrapper_sha256: String,
    pub native_boundary: bool,
    pub exit_zero_preserved: bool,
    pub exit_seven_preserved: bool,
    pub completion_pairing: bool,
    pub cleanup_complete: bool,
    pub detail: serde_json::Value,
}

type RecordedLifecycle = (
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);
type ReadinessLifecycle = (
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

fn now_ms() -> Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| CoreError::Integrity("system clock precedes Unix epoch".to_string()))?
        .as_millis()
        .try_into()
        .map_err(|_| CoreError::Integrity("millisecond timestamp overflow".to_string()))
}

fn hash_id(parts: &[&str]) -> String {
    let mut bytes = Vec::new();
    for part in parts {
        bytes.extend_from_slice(&(part.len() as u64).to_be_bytes());
        bytes.extend_from_slice(part.as_bytes());
    }
    sha256_hex(&bytes)
}

pub fn create_install_plan(ledger: &Ledger, input: InstallPlanInput) -> Result<InstallPlan> {
    if input.owner_key.is_empty()
        || !matches!(input.source.as_str(), "codex" | "claude_code")
        || input.matcher.is_empty()
    {
        return Err(CoreError::Config(
            "install plan requires owner, recognized source, and matcher".to_string(),
        ));
    }
    let created_at_unix_ms = now_ms()?;
    let identity_json = canonical_json(&input)?;
    let plan_id = hash_id(&[
        &sha256_hex(&identity_json),
        &created_at_unix_ms.to_string(),
        "m5r2-install-plan-v1",
    ]);
    let plan = InstallPlan {
        schema_version: M5R2_PROFILE_SCHEMA_VERSION,
        plan_id,
        owner_key: input.owner_key,
        source: input.source,
        config_identity: input.config_identity,
        config_preimage_sha256: input.config_preimage_sha256,
        proposed_config_sha256: input.proposed_config_sha256,
        source_version: input.source_version,
        source_executable_sha256: input.source_executable_sha256,
        wrapper_sha256: input.wrapper_sha256,
        matcher: input.matcher,
        proposed_config: input.proposed_config,
        created_at_unix_ms,
    };
    let plan_sha = sha256_hex(&canonical_json(&plan)?);
    ledger.conn().execute(
        "INSERT INTO capture_install_plans(
           plan_id,owner_key,source,config_identity,config_preimage_sha256,
           proposed_config_sha256,source_version,source_executable_sha256,
           wrapper_sha256,matcher_json,proposed_config_json,plan_sha256,created_at_unix_ms)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
        params![
            plan.plan_id,
            plan.owner_key,
            plan.source,
            plan.config_identity,
            plan.config_preimage_sha256,
            plan.proposed_config_sha256,
            plan.source_version,
            plan.source_executable_sha256,
            plan.wrapper_sha256,
            serde_json::to_string(&plan.matcher)?,
            serde_json::to_string(&plan.proposed_config)?,
            plan_sha,
            plan.created_at_unix_ms as i64,
        ],
    )?;
    Ok(plan)
}

pub fn load_install_plan(ledger: &Ledger, plan_id: &str) -> Result<InstallPlan> {
    let (plan, expected_sha): (InstallPlan, String) = ledger
        .conn()
        .query_row(
            "SELECT owner_key,source,config_identity,config_preimage_sha256,
                    proposed_config_sha256,source_version,source_executable_sha256,
                    wrapper_sha256,matcher_json,proposed_config_json,created_at_unix_ms,plan_sha256
             FROM capture_install_plans WHERE plan_id=?1",
            params![plan_id],
            |row| {
                let matcher_json: String = row.get(8)?;
                let proposed_json: String = row.get(9)?;
                let matcher = serde_json::from_str(&matcher_json).map_err(json_sql_error)?;
                let proposed_config =
                    serde_json::from_str(&proposed_json).map_err(json_sql_error)?;
                Ok((
                    InstallPlan {
                        schema_version: M5R2_PROFILE_SCHEMA_VERSION,
                        plan_id: plan_id.to_string(),
                        owner_key: row.get(0)?,
                        source: row.get(1)?,
                        config_identity: row.get(2)?,
                        config_preimage_sha256: row.get(3)?,
                        proposed_config_sha256: row.get(4)?,
                        source_version: row.get(5)?,
                        source_executable_sha256: row.get(6)?,
                        wrapper_sha256: row.get(7)?,
                        matcher,
                        proposed_config,
                        created_at_unix_ms: row.get::<_, i64>(10)? as u64,
                    },
                    row.get(11)?,
                ))
            },
        )
        .map_err(CoreError::from)?;
    if sha256_hex(&canonical_json(&plan)?) != expected_sha {
        return Err(CoreError::Integrity(
            "stored installation plan digest mismatch".to_string(),
        ));
    }
    Ok(plan)
}

pub fn record_lifecycle(
    ledger: &Ledger,
    collector: &Collector,
    payload: &LifecyclePayload,
) -> Result<String> {
    if payload.owner_key != collector.owner_key() || payload.source != collector.source() {
        return Err(CoreError::Integrity(
            "lifecycle record crosses collector scope".to_string(),
        ));
    }
    if !matches!(
        payload.action.as_str(),
        "installed" | "enabled" | "disabled" | "uninstalled" | "recovered" | "observation_failed"
    ) {
        return Err(CoreError::Config(
            "unknown capture lifecycle action".to_string(),
        ));
    }
    let (digest, signature) = collector.sign_record(payload)?;
    let id = hash_id(&[&digest, collector.key_id(), "m5r2-lifecycle-v1"]);
    let tx = ledger.conn().unchecked_transaction()?;
    ensure_collector_key(&tx, collector, payload.accepted_at_unix_ms)?;
    let seq: i64 = tx.query_row(
        "SELECT COALESCE(MAX(accepted_seq),0)+1 FROM capture_lifecycle_records",
        [],
        |row| row.get(0),
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO capture_lifecycle_records(
           record_id,owner_key,source,action,supersedes_record_id,plan_id,config_identity,config_sha256,
           source_version,source_executable_sha256,wrapper_sha256,collector_key_id,
           detail_json,record_sha256,key_id,signature_hex,accepted_seq,accepted_at_unix_ms)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)",
        params![
            id,
            payload.owner_key,
            payload.source,
            payload.action,
            payload.supersedes_record_id,
            payload.plan_id,
            payload.config_identity,
            payload.config_sha256,
            payload.source_version,
            payload.source_executable_sha256,
            payload.wrapper_sha256,
            payload.collector_key_id,
            serde_json::to_string(&payload.detail)?,
            digest,
            collector.key_id(),
            signature,
            seq,
            payload.accepted_at_unix_ms as i64,
        ],
    )?;
    tx.commit()?;
    Ok(id)
}

pub fn new_lifecycle_payload(
    collector: &Collector,
    action: &str,
    plan: Option<&InstallPlan>,
    config_identity: String,
    detail: serde_json::Value,
) -> Result<LifecyclePayload> {
    Ok(LifecyclePayload {
        schema_version: M5R2_PROFILE_SCHEMA_VERSION,
        owner_key: collector.owner_key().to_string(),
        source: collector.source().to_string(),
        action: action.to_string(),
        supersedes_record_id: None,
        plan_id: plan.map(|value| value.plan_id.clone()),
        config_identity,
        config_sha256: plan.map(|value| value.proposed_config_sha256.clone()),
        source_version: plan.map(|value| value.source_version.clone()),
        source_executable_sha256: plan.map(|value| value.source_executable_sha256.clone()),
        wrapper_sha256: plan.map(|value| value.wrapper_sha256.clone()),
        collector_key_id: Some(collector.key_id().to_string()),
        detail,
        accepted_at_unix_ms: now_ms()?,
    })
}

pub fn record_authorization(
    ledger: &Ledger,
    collector: &Collector,
    source_project_key: &str,
    authorized: bool,
    reason: &str,
) -> Result<String> {
    if source_project_key.is_empty() || reason.trim().is_empty() {
        return Err(CoreError::Config(
            "authorization requires project identity and reason".to_string(),
        ));
    }
    let payload = AuthorizationPayload {
        schema_version: M5R2_PROFILE_SCHEMA_VERSION,
        owner_key: collector.owner_key().to_string(),
        source: collector.source().to_string(),
        source_project_key: source_project_key.to_string(),
        action: if authorized { "authorized" } else { "revoked" }.to_string(),
        reason: reason.to_string(),
        accepted_at_unix_ms: now_ms()?,
    };
    let (digest, signature) = collector.sign_record(&payload)?;
    let id = hash_id(&[&digest, collector.key_id(), "m5r2-authorization-v1"]);
    let tx = ledger.conn().unchecked_transaction()?;
    ensure_collector_key(&tx, collector, payload.accepted_at_unix_ms)?;
    let seq: i64 = tx.query_row(
        "SELECT COALESCE(MAX(accepted_seq),0)+1 FROM capture_project_authorizations",
        [],
        |row| row.get(0),
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO capture_project_authorizations(
           authorization_id,owner_key,source,source_project_key,action,reason,
           record_sha256,key_id,signature_hex,accepted_seq,accepted_at_unix_ms)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        params![
            id,
            payload.owner_key,
            payload.source,
            payload.source_project_key,
            payload.action,
            payload.reason,
            digest,
            collector.key_id(),
            signature,
            seq,
            payload.accepted_at_unix_ms as i64,
        ],
    )?;
    tx.commit()?;
    Ok(id)
}

pub fn project_authorized(
    ledger: &Ledger,
    owner: &str,
    source: &str,
    project: &str,
) -> Result<bool> {
    let action: Option<String> = ledger
        .conn()
        .query_row(
            "SELECT action FROM capture_project_authorizations
             WHERE owner_key=?1 AND source=?2 AND source_project_key=?3
             ORDER BY accepted_seq DESC LIMIT 1",
            params![owner, source, project],
            |row| row.get(0),
        )
        .optional()?;
    Ok(action.as_deref() == Some("authorized"))
}

pub fn record_self_test(
    ledger: &Ledger,
    collector: &Collector,
    payload: &SelfTestPayload,
) -> Result<String> {
    if payload.owner_key != collector.owner_key()
        || payload.source != collector.source()
        || payload.collector_key_id != collector.key_id()
    {
        return Err(CoreError::Integrity(
            "self-test record crosses collector scope".to_string(),
        ));
    }
    let (digest, signature) = collector.sign_record(payload)?;
    let id = hash_id(&[&digest, collector.key_id(), "m5r2-self-test-v1"]);
    let tx = ledger.conn().unchecked_transaction()?;
    ensure_collector_key(&tx, collector, payload.accepted_at_unix_ms)?;
    let seq: i64 = tx.query_row(
        "SELECT COALESCE(MAX(accepted_seq),0)+1 FROM capture_self_tests",
        [],
        |row| row.get(0),
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO capture_self_tests(
           self_test_id,owner_key,source,source_version,source_executable_sha256,
           config_sha256,wrapper_sha256,collector_key_id,native_boundary,
           exit_zero_preserved,exit_seven_preserved,completion_pairing,cleanup_complete,
           passed,detail_json,record_sha256,key_id,signature_hex,accepted_seq,accepted_at_unix_ms)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)",
        params![
            id,
            payload.owner_key,
            payload.source,
            payload.source_version,
            payload.source_executable_sha256,
            payload.config_sha256,
            payload.wrapper_sha256,
            payload.collector_key_id,
            payload.native_boundary as i64,
            payload.exit_zero_preserved as i64,
            payload.exit_seven_preserved as i64,
            payload.completion_pairing as i64,
            payload.cleanup_complete as i64,
            payload.passed() as i64,
            serde_json::to_string(&payload.detail)?,
            digest,
            collector.key_id(),
            signature,
            seq,
            payload.accepted_at_unix_ms as i64,
        ],
    )?;
    tx.commit()?;
    Ok(id)
}

pub fn record_compatibility_profile(
    ledger: &Ledger,
    collector: &Collector,
    payload: &CompatibilityProfilePayload,
) -> Result<String> {
    if payload.owner_key != collector.owner_key()
        || payload.source != collector.source()
        || payload.collector_key_id != collector.key_id()
        || payload.source != "codex"
        || payload.source_version != QUALIFIED_CODEX_VERSION
        || payload.matcher != ["Bash"]
        || payload.capsule_schema_version != CAPSULE_SCHEMA_VERSION
        || !payload.permissions_valid
    {
        return Err(CoreError::Integrity(
            "compatibility profile is not eligible for Codex assurance".to_string(),
        ));
    }
    let self_test_passed: Option<i64> = ledger
        .conn()
        .query_row(
            "SELECT passed FROM capture_self_tests WHERE self_test_id=?1
             AND owner_key=?2 AND source=?3 AND source_version=?4
             AND source_executable_sha256=?5 AND config_sha256=?6
             AND wrapper_sha256=?7 AND collector_key_id=?8",
            params![
                payload.self_test_id,
                payload.owner_key,
                payload.source,
                payload.source_version,
                payload.source_executable_sha256,
                payload.config_sha256,
                payload.wrapper_sha256,
                payload.collector_key_id,
            ],
            |row| row.get(0),
        )
        .optional()?;
    if self_test_passed != Some(1) {
        return Err(CoreError::Integrity(
            "compatibility profile has no matching passed self-test".to_string(),
        ));
    }
    let (digest, signature) = collector.sign_record(payload)?;
    let id = hash_id(&[&digest, collector.key_id(), "m5r2-profile-v1"]);
    let tx = ledger.conn().unchecked_transaction()?;
    ensure_collector_key(&tx, collector, payload.accepted_at_unix_ms)?;
    let seq: i64 = tx.query_row(
        "SELECT COALESCE(MAX(accepted_seq),0)+1 FROM capture_compatibility_profiles",
        [],
        |row| row.get(0),
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO capture_compatibility_profiles(
           profile_id,owner_key,source,source_version,source_executable_sha256,
           config_sha256,matcher_json,wrapper_sha256,capsule_schema_version,
           collector_key_id,permissions_valid,self_test_id,measured_boundary_ref,
           supersedes_profile_id,profile_json,profile_sha256,key_id,signature_hex,accepted_seq,accepted_at_unix_ms)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)",
        params![
            id,
            payload.owner_key,
            payload.source,
            payload.source_version,
            payload.source_executable_sha256,
            payload.config_sha256,
            serde_json::to_string(&payload.matcher)?,
            payload.wrapper_sha256,
            payload.capsule_schema_version as i64,
            payload.collector_key_id,
            payload.permissions_valid as i64,
            payload.self_test_id,
            payload.measured_boundary_ref,
            payload.supersedes_profile_id,
            String::from_utf8(canonical_json(payload)?).map_err(|_| {
                CoreError::Integrity("canonical profile was not UTF-8".to_string())
            })?,
            digest,
            collector.key_id(),
            signature,
            seq,
            payload.accepted_at_unix_ms as i64,
        ],
    )?;
    tx.commit()?;
    Ok(id)
}

pub fn new_self_test_payload(
    collector: &Collector,
    input: NewSelfTestInput,
) -> Result<SelfTestPayload> {
    Ok(SelfTestPayload {
        schema_version: M5R2_PROFILE_SCHEMA_VERSION,
        owner_key: collector.owner_key().to_string(),
        source: collector.source().to_string(),
        source_version: input.source_version,
        source_executable_sha256: input.source_executable_sha256,
        config_sha256: input.config_sha256,
        wrapper_sha256: input.wrapper_sha256,
        collector_key_id: collector.key_id().to_string(),
        native_boundary: input.native_boundary,
        exit_zero_preserved: input.exit_zero_preserved,
        exit_seven_preserved: input.exit_seven_preserved,
        completion_pairing: input.completion_pairing,
        cleanup_complete: input.cleanup_complete,
        detail: input.detail,
        accepted_at_unix_ms: now_ms()?,
    })
}

pub fn new_compatibility_profile(
    test: &SelfTestPayload,
    self_test_id: String,
    permissions_valid: bool,
) -> Result<CompatibilityProfilePayload> {
    Ok(CompatibilityProfilePayload {
        schema_version: M5R2_PROFILE_SCHEMA_VERSION,
        owner_key: test.owner_key.clone(),
        source: test.source.clone(),
        source_version: test.source_version.clone(),
        source_executable_sha256: test.source_executable_sha256.clone(),
        config_sha256: test.config_sha256.clone(),
        matcher: vec!["Bash".to_string()],
        wrapper_sha256: test.wrapper_sha256.clone(),
        capsule_schema_version: CAPSULE_SCHEMA_VERSION,
        collector_key_id: test.collector_key_id.clone(),
        permissions_valid,
        self_test_id,
        measured_boundary_ref: M5R1_BOUNDARY_REFERENCE.to_string(),
        supersedes_profile_id: None,
        accepted_at_unix_ms: now_ms()?,
    })
}

pub fn operational_assurance_enabled(
    ledger: &Ledger,
    input: &CaptureReadinessInput,
) -> Result<bool> {
    Ok(derive_readiness(ledger, input)?.state == CaptureReadiness::AssuredReady)
}

/// Reconstruct the last accepted operational view without touching the live
/// source, configuration, collector filesystem, Git, or the network. Exports
/// use this bounded view; live drift appears only after an explicit accepted
/// lifecycle action or observation records it.
pub fn recorded_readiness(
    ledger: &Ledger,
    owner: &str,
    source: &str,
    source_project_key: &str,
) -> Result<CaptureReadinessReport> {
    if source != "codex" {
        let input = CaptureReadinessInput {
            owner_key: owner.to_string(),
            source: source.to_string(),
            source_project_key: Some(source_project_key.to_string()),
            source_present: true,
            source_version: None,
            source_executable_sha256: None,
            config_identity: "accepted-records".to_string(),
            config_sha256: None,
            wrapper_sha256: None,
            collector_key_id: None,
            collector_permissions_valid: false,
            matcher_valid: false,
            duplicate_owned_entries: false,
            source_pairing_complete: false,
            selected_command_profile_supported: false,
            latest_capture_error: false,
        };
        return report(
            ledger,
            &input,
            CaptureReadiness::BoundedOnly,
            "This source can observe state but cannot prove the command boundary.",
            None,
            None,
        );
    }
    let lifecycle: Option<RecordedLifecycle> = ledger
        .conn()
        .query_row(
            "SELECT action,config_identity,config_sha256,source_version,
                    source_executable_sha256,wrapper_sha256,collector_key_id
             FROM capture_lifecycle_records
             WHERE owner_key=?1 AND source=?2 ORDER BY accepted_seq DESC LIMIT 1",
            params![owner, source],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .optional()?;
    let Some((action, config_identity, config, version, executable, wrapper, key)) = lifecycle
    else {
        let input = CaptureReadinessInput {
            owner_key: owner.to_string(),
            source: source.to_string(),
            source_project_key: Some(source_project_key.to_string()),
            source_present: true,
            source_version: None,
            source_executable_sha256: None,
            config_identity: "accepted-records".to_string(),
            config_sha256: None,
            wrapper_sha256: None,
            collector_key_id: None,
            collector_permissions_valid: false,
            matcher_valid: false,
            duplicate_owned_entries: false,
            source_pairing_complete: false,
            selected_command_profile_supported: true,
            latest_capture_error: false,
        };
        return report(
            ledger,
            &input,
            CaptureReadiness::AvailableNotInstalled,
            "Capture is available but no accepted installation exists.",
            None,
            None,
        );
    };
    let latest_attempt: Option<(String, String)> = ledger
        .conn()
        .query_row(
            "SELECT status,profile FROM capture_attempts
             WHERE owner_key=?1 AND source=?2 AND source_project_key=?3
             ORDER BY created_at_unix_ms DESC,attempt_id DESC LIMIT 1",
            params![owner, source, source_project_key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let input = CaptureReadinessInput {
        owner_key: owner.to_string(),
        source: source.to_string(),
        source_project_key: Some(source_project_key.to_string()),
        source_present: true,
        source_version: version,
        source_executable_sha256: executable,
        config_identity,
        config_sha256: config,
        wrapper_sha256: wrapper,
        collector_key_id: key,
        collector_permissions_valid: true,
        matcher_valid: true,
        duplicate_owned_entries: false,
        source_pairing_complete: latest_attempt
            .as_ref()
            .is_none_or(|(status, _)| status == "accepted"),
        selected_command_profile_supported: latest_attempt
            .as_ref()
            .is_none_or(|(_, profile)| !profile.starts_with("unrecognized")),
        latest_capture_error: latest_attempt
            .as_ref()
            .is_some_and(|(status, _)| status != "accepted"),
    };
    let mut result = derive_readiness(ledger, &input)?;
    if action == "uninstalled" {
        result.reason =
            "Managed hooks were uninstalled; historical evidence is retained.".to_string();
    }
    Ok(result)
}

pub fn derive_readiness(
    ledger: &Ledger,
    input: &CaptureReadinessInput,
) -> Result<CaptureReadinessReport> {
    if !input.source_present {
        return report(
            ledger,
            input,
            CaptureReadiness::NotAvailable,
            "Assured Capture is not available for this source.",
            None,
            None,
        );
    }
    if input.source != "codex" {
        return report(
            ledger,
            input,
            CaptureReadiness::BoundedOnly,
            "This source can observe state but cannot prove the command boundary.",
            None,
            None,
        );
    }
    let lifecycle: Option<ReadinessLifecycle> = ledger
        .conn()
        .query_row(
            "SELECT action,config_sha256,source_version,source_executable_sha256,
                    wrapper_sha256,collector_key_id
             FROM capture_lifecycle_records
             WHERE owner_key=?1 AND source=?2 AND config_identity=?3
             ORDER BY accepted_seq DESC LIMIT 1",
            params![input.owner_key, input.source, input.config_identity],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .optional()?;
    let (
        action,
        expected_config,
        expected_version,
        expected_executable,
        expected_wrapper,
        expected_key,
    ) = match lifecycle {
        Some(values) => values,
        None => {
            return report(
                ledger,
                input,
                CaptureReadiness::AvailableNotInstalled,
                "Capture is available but not installed; no project is authorized.",
                None,
                None,
            )
        }
    };
    if action == "uninstalled" {
        return report(
            ledger,
            input,
            CaptureReadiness::AvailableNotInstalled,
            "Managed source hooks were uninstalled; historical evidence is retained.",
            None,
            Some(action),
        );
    }
    if action == "disabled" {
        return report(
            ledger,
            input,
            CaptureReadiness::Disabled,
            "Capture is disabled by an explicit local receipt.",
            None,
            Some(action),
        );
    }
    if input.source_version != expected_version
        || input.source_executable_sha256 != expected_executable
    {
        return report(
            ledger,
            input,
            CaptureReadiness::DegradedSourceVersion,
            "The source version or executable is not qualified; earlier proof is retained.",
            None,
            Some(action),
        );
    }
    if input.config_sha256 != expected_config
        || input.wrapper_sha256 != expected_wrapper
        || !input.matcher_valid
        || input.duplicate_owned_entries
    {
        return report(
            ledger,
            input,
            CaptureReadiness::DegradedConfigurationDrift,
            "Managed capture configuration, matcher, or wrapper drifted; new assurance is off.",
            None,
            Some(action),
        );
    }
    if !input.collector_permissions_valid || input.collector_key_id != expected_key {
        return report(
            ledger,
            input,
            CaptureReadiness::DegradedKeyOrIdentity,
            "The collector key or private permissions are invalid.",
            None,
            Some(action),
        );
    }
    let test_passed: Option<i64> = ledger
        .conn()
        .query_row(
            "SELECT passed FROM capture_self_tests
         WHERE owner_key=?1 AND source=?2 AND source_version=?3
           AND source_executable_sha256=?4 AND config_sha256=?5
           AND wrapper_sha256=?6 AND collector_key_id=?7
         ORDER BY accepted_seq DESC LIMIT 1",
            params![
                input.owner_key,
                input.source,
                input.source_version,
                input.source_executable_sha256,
                input.config_sha256,
                input.wrapper_sha256,
                input.collector_key_id
            ],
            |row| row.get(0),
        )
        .optional()?;
    if test_passed != Some(1) {
        return report(
            ledger,
            input,
            CaptureReadiness::InstalledUnverified,
            "Installed; a matching native-boundary self-test is required.",
            None,
            Some(action),
        );
    }
    let profile = matching_verified_profile(ledger, input)?;
    let profile_id = match profile {
        Some(value) => value,
        None => {
            return report(
                ledger,
                input,
                CaptureReadiness::InstalledUnverified,
                "Installed; a signed profile from a complete self-test is required.",
                None,
                Some(action),
            )
        }
    };
    let project = match input.source_project_key.as_deref() {
        Some(value) => value,
        None => {
            return report(
                ledger,
                input,
                CaptureReadiness::InstalledUnverified,
                "Installed and tested; select and explicitly authorize a project.",
                Some(profile_id),
                Some(action),
            )
        }
    };
    if !project_authorized(ledger, &input.owner_key, &input.source, project)? {
        return report(
            ledger,
            input,
            CaptureReadiness::InstalledUnverified,
            "Installed and tested; this source project is not authorized.",
            Some(profile_id),
            Some(action),
        );
    }
    if !input.source_pairing_complete {
        return report(
            ledger,
            input,
            CaptureReadiness::DegradedSourcePairing,
            "The latest native delivery has not paired to an accepted event.",
            Some(profile_id),
            Some(action),
        );
    }
    if !input.selected_command_profile_supported {
        return report(
            ledger,
            input,
            CaptureReadiness::UnsupportedProfile,
            "The selected command or repository surface is outside the admitted profile.",
            Some(profile_id),
            Some(action),
        );
    }
    if input.latest_capture_error {
        return report(
            ledger,
            input,
            CaptureReadiness::CaptureError,
            "The latest eligible capture did not complete; earlier proof is unchanged.",
            Some(profile_id),
            Some(action),
        );
    }
    report(
        ledger,
        input,
        CaptureReadiness::AssuredReady,
        "Exact profile, self-test, authorization, and pairing gates are satisfied.",
        Some(profile_id),
        Some(action),
    )
}

fn matching_verified_profile(
    ledger: &Ledger,
    input: &CaptureReadinessInput,
) -> Result<Option<String>> {
    let mut statement = ledger.conn().prepare(
        "SELECT p.profile_id,p.profile_json,p.profile_sha256,p.signature_hex,k.public_key_hex
         FROM capture_compatibility_profiles p JOIN collector_keys k ON k.key_id=p.key_id
         WHERE p.owner_key=?1 AND p.source=?2 AND p.source_version=?3
           AND p.source_executable_sha256=?4 AND p.config_sha256=?5
           AND p.wrapper_sha256=?6 AND p.collector_key_id=?7
           AND p.permissions_valid=1 AND p.capsule_schema_version=?8
         ORDER BY p.accepted_seq DESC",
    )?;
    let rows = statement.query_map(
        params![
            input.owner_key,
            input.source,
            input.source_version,
            input.source_executable_sha256,
            input.config_sha256,
            input.wrapper_sha256,
            input.collector_key_id,
            CAPSULE_SCHEMA_VERSION as i64
        ],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        },
    )?;
    for row in rows {
        let (id, json, digest, signature, public) = row?;
        let payload: CompatibilityProfilePayload = match serde_json::from_str(&json) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if payload.matcher == ["Bash"]
            && verify_signed_record(&payload, &digest, &signature, &public)?
        {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

fn report(
    ledger: &Ledger,
    input: &CaptureReadinessInput,
    state: CaptureReadiness,
    reason: &str,
    profile_id: Option<String>,
    latest_lifecycle_action: Option<String>,
) -> Result<CaptureReadinessReport> {
    let (count, latest): (i64, Option<i64>) = ledger.conn().query_row(
        "SELECT COUNT(*),MAX(created_at_unix_ms) FROM capture_attempts
         WHERE owner_key=?1 AND source=?2 AND status='accepted'",
        params![input.owner_key, input.source],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let incomplete: i64 = ledger.conn().query_row(
        "SELECT COUNT(*) FROM capture_attempts
         WHERE owner_key=?1 AND source=?2 AND status!='accepted'",
        params![input.owner_key, input.source],
        |row| row.get(0),
    )?;
    let mut proof_currency = BTreeMap::new();
    let mut currency_statement = ledger.conn().prepare(
        "SELECT c.currentness,COUNT(*) FROM state_comparisons c
         JOIN state_capsules s ON s.capsule_id=c.execution_capsule_id
         JOIN capture_attempts a ON a.attempt_id=s.attempt_id
         WHERE a.owner_key=?1 AND a.source=?2 GROUP BY c.currentness ORDER BY c.currentness",
    )?;
    let currency_rows = currency_statement
        .query_map(params![input.owner_key, input.source], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
    for row in currency_rows {
        let (currency, total) = row?;
        proof_currency.insert(currency, total.max(0) as u64);
    }
    let project_authorized = match input.source_project_key.as_deref() {
        Some(project) => project_authorized(ledger, &input.owner_key, &input.source, project)?,
        None => false,
    };
    Ok(CaptureReadinessReport {
        state,
        reason: reason.to_string(),
        owner_key: input.owner_key.clone(),
        source: input.source.clone(),
        source_project_key: input.source_project_key.clone(),
        profile_id,
        latest_lifecycle_action,
        accepted_capture_count: count as u64,
        incomplete_capture_count: incomplete.max(0) as u64,
        latest_capture_at_unix_ms: latest.map(|value| value as u64),
        source_available: input.source_present,
        matcher_valid: input.matcher_valid,
        duplicate_owned_entries: input.duplicate_owned_entries,
        collector_key_available: input.collector_key_id.is_some(),
        collector_permissions_valid: input.collector_permissions_valid,
        source_pairing_complete: input.source_pairing_complete,
        selected_command_profile_supported: input.selected_command_profile_supported,
        project_authorized,
        proof_currency,
        historical_evidence_retained: true,
    })
}

fn ensure_collector_key(
    tx: &rusqlite::Transaction<'_>,
    collector: &Collector,
    now: u64,
) -> Result<()> {
    tx.execute(
        "INSERT OR IGNORE INTO collector_keys(
           key_id,owner_key,source,algorithm,public_key_hex,created_at_unix_ms)
         VALUES(?1,?2,?3,'ed25519',?4,?5)",
        params![
            collector.key_id(),
            collector.owner_key(),
            collector.source(),
            collector.public_key_hex(),
            now as i64
        ],
    )?;
    Ok(())
}

fn json_sql_error(error: serde_json::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn base_input() -> CaptureReadinessInput {
        CaptureReadinessInput {
            owner_key: "owner".to_string(),
            source: "codex".to_string(),
            source_project_key: Some("project".to_string()),
            source_present: true,
            source_version: Some(QUALIFIED_CODEX_VERSION.to_string()),
            source_executable_sha256: Some("source-exe".to_string()),
            config_identity: "config".to_string(),
            config_sha256: Some("config-sha".to_string()),
            wrapper_sha256: Some("wrapper-sha".to_string()),
            collector_key_id: None,
            collector_permissions_valid: true,
            matcher_valid: true,
            duplicate_owned_entries: false,
            source_pairing_complete: true,
            selected_command_profile_supported: true,
            latest_capture_error: false,
        }
    }

    fn ready_fixture() -> (TempDir, Ledger, Collector, CaptureReadinessInput) {
        let temp = TempDir::new().unwrap();
        let ledger = Ledger::open(temp.path().join("ledger.db")).unwrap();
        let collector = Collector::open(temp.path().join("capture"), "owner", "codex").unwrap();
        let plan = create_install_plan(
            &ledger,
            InstallPlanInput {
                owner_key: "owner".to_string(),
                source: "codex".to_string(),
                config_identity: "config".to_string(),
                config_preimage_sha256: "before".to_string(),
                proposed_config_sha256: "after".to_string(),
                source_version: QUALIFIED_CODEX_VERSION.to_string(),
                source_executable_sha256: "source-exe".to_string(),
                wrapper_sha256: "wrapper-sha".to_string(),
                matcher: vec!["Bash".to_string()],
                proposed_config: serde_json::json!({"hooks": []}),
            },
        )
        .unwrap();
        let lifecycle = new_lifecycle_payload(
            &collector,
            "installed",
            Some(&plan),
            "config".to_string(),
            serde_json::json!({}),
        )
        .unwrap();
        record_lifecycle(&ledger, &collector, &lifecycle).unwrap();
        let test = new_self_test_payload(
            &collector,
            NewSelfTestInput {
                source_version: QUALIFIED_CODEX_VERSION.to_string(),
                source_executable_sha256: "source-exe".to_string(),
                config_sha256: "after".to_string(),
                wrapper_sha256: "wrapper-sha".to_string(),
                native_boundary: true,
                exit_zero_preserved: true,
                exit_seven_preserved: true,
                completion_pairing: true,
                cleanup_complete: true,
                detail: serde_json::json!({"native": true}),
            },
        )
        .unwrap();
        let test_id = record_self_test(&ledger, &collector, &test).unwrap();
        let profile = new_compatibility_profile(&test, test_id, true).unwrap();
        record_compatibility_profile(&ledger, &collector, &profile).unwrap();
        record_authorization(&ledger, &collector, "project", true, "test authorization").unwrap();
        let mut input = base_input();
        input.config_sha256 = Some("after".to_string());
        input.collector_key_id = Some(collector.key_id().to_string());
        (temp, ledger, collector, input)
    }

    #[test]
    fn readiness_is_not_installed_without_accepted_lifecycle() {
        let temp = TempDir::new().unwrap();
        let ledger = Ledger::open(temp.path().join("ledger.db")).unwrap();
        let report = derive_readiness(&ledger, &base_input()).unwrap();
        assert_eq!(report.state, CaptureReadiness::AvailableNotInstalled);
    }

    #[test]
    fn install_plans_are_immutable_and_digest_checked() {
        let temp = TempDir::new().unwrap();
        let ledger = Ledger::open(temp.path().join("ledger.db")).unwrap();
        let plan = create_install_plan(
            &ledger,
            InstallPlanInput {
                owner_key: "owner".to_string(),
                source: "codex".to_string(),
                config_identity: "config".to_string(),
                config_preimage_sha256: "before".to_string(),
                proposed_config_sha256: "after".to_string(),
                source_version: QUALIFIED_CODEX_VERSION.to_string(),
                source_executable_sha256: "source".to_string(),
                wrapper_sha256: "wrapper".to_string(),
                matcher: vec!["Bash".to_string()],
                proposed_config: serde_json::json!({"hooks": []}),
            },
        )
        .unwrap();
        assert_eq!(load_install_plan(&ledger, &plan.plan_id).unwrap(), plan);
        assert!(ledger
            .conn()
            .execute(
                "UPDATE capture_install_plans SET source_version='drift' WHERE plan_id=?1",
                params![plan.plan_id],
            )
            .is_err());
    }

    #[test]
    fn canonical_operational_state_precedence_covers_all_twelve_states() {
        let (_temp, ledger, collector, ready) = ready_fixture();
        assert_eq!(
            derive_readiness(&ledger, &ready).unwrap().state,
            CaptureReadiness::AssuredReady
        );

        let mut input = ready.clone();
        input.source_present = false;
        assert_eq!(
            derive_readiness(&ledger, &input).unwrap().state,
            CaptureReadiness::NotAvailable
        );
        input = ready.clone();
        input.source = "claude_code".to_string();
        assert_eq!(
            derive_readiness(&ledger, &input).unwrap().state,
            CaptureReadiness::BoundedOnly
        );
        input = ready.clone();
        input.source_version = Some("newer".to_string());
        assert_eq!(
            derive_readiness(&ledger, &input).unwrap().state,
            CaptureReadiness::DegradedSourceVersion
        );
        input = ready.clone();
        input.matcher_valid = false;
        assert_eq!(
            derive_readiness(&ledger, &input).unwrap().state,
            CaptureReadiness::DegradedConfigurationDrift
        );
        input = ready.clone();
        input.collector_key_id = Some("replacement".to_string());
        assert_eq!(
            derive_readiness(&ledger, &input).unwrap().state,
            CaptureReadiness::DegradedKeyOrIdentity
        );
        input = ready.clone();
        input.source_project_key = Some("not-authorized".to_string());
        assert_eq!(
            derive_readiness(&ledger, &input).unwrap().state,
            CaptureReadiness::InstalledUnverified
        );
        input = ready.clone();
        input.source_pairing_complete = false;
        assert_eq!(
            derive_readiness(&ledger, &input).unwrap().state,
            CaptureReadiness::DegradedSourcePairing
        );
        input = ready.clone();
        input.selected_command_profile_supported = false;
        assert_eq!(
            derive_readiness(&ledger, &input).unwrap().state,
            CaptureReadiness::UnsupportedProfile
        );
        input = ready.clone();
        input.latest_capture_error = true;
        assert_eq!(
            derive_readiness(&ledger, &input).unwrap().state,
            CaptureReadiness::CaptureError
        );

        let empty = TempDir::new().unwrap();
        let empty_ledger = Ledger::open(empty.path().join("ledger.db")).unwrap();
        assert_eq!(
            derive_readiness(&empty_ledger, &base_input())
                .unwrap()
                .state,
            CaptureReadiness::AvailableNotInstalled
        );

        let disabled = new_lifecycle_payload(
            &collector,
            "disabled",
            None,
            "config".to_string(),
            serde_json::json!({"reason": "test"}),
        )
        .unwrap();
        record_lifecycle(&ledger, &collector, &disabled).unwrap();
        assert_eq!(
            derive_readiness(&ledger, &ready).unwrap().state,
            CaptureReadiness::Disabled
        );
    }
}
