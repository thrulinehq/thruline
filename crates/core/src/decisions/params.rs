//! Closed, versioned parameters for person-accepted standing checks.
//!
//! Unknown fields are errors. In particular, a source comment cannot add a
//! hidden allowlist: only the accepted parameter document carries exclusions.

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

use super::checks_preservation::{
    AssertionFloorParams, CiStepPresentParams, RuleFilePresentParams,
};
use super::checks_process::{
    CommandForbiddenParams, ConsentRecordedParams, OneHeavyJobParams, TestsBeforeCodeParams,
};

pub const PARAMS_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    Code,
    UiCopy,
    Docs,
    Tests,
    Ci,
    Other,
}

impl Surface {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Code => "code",
            Self::UiCopy => "ui_copy",
            Self::Docs => "docs",
            Self::Tests => "tests",
            Self::Ci => "ci",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextAbsentParams {
    pub terms: Vec<String>,
    pub synonyms: Vec<String>,
    pub surfaces: Vec<Surface>,
    pub allowlist: Vec<String>,
    #[serde(default)]
    pub test_allowlist: Vec<super::text_exclusions::TestScope>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub json_field_allowlist: Vec<super::c31_json_field::JsonFieldScope>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditional_path_allowlist: Vec<super::c31_archive::ArchiveAbsentScope>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub span_allowlist: Vec<super::c32_span_scope::SpanScope>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SymbolAbsentParams {
    pub symbol: String,
    pub language: Option<String>,
    pub surfaces: Vec<Surface>,
    pub allowlist: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiCopyAbsentParams {
    pub terms: Vec<String>,
    pub allowlist: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathPresentParams {
    pub path: String,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestPresentAndRunsParams {
    pub name_pattern: String,
    pub command: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureDefaultPreservedParams {
    pub path: String,
    pub symbol: String,
    pub expected_value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditsWithinPathsParams {
    pub allowed_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathsUntouchedParams {
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TermEverywhereParams {
    pub canonical: String,
    pub forbidden_variants: Vec<String>,
    pub surfaces: Vec<Surface>,
    pub allowlist: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForbiddenWordsParams {
    pub words: Vec<String>,
    pub surfaces: Vec<Surface>,
    pub allowlist: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandPassesParams {
    pub command: String,
    pub is_test: bool,
    pub min_passed: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ByteSpan {
    pub start: u64,
    pub end: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileEqualsSpan {
    pub left: Option<ByteSpan>,
    pub right: Option<ByteSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileEqualsParams {
    pub left: String,
    pub right: String,
    pub span: Option<FileEqualsSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoldenOutputParams {
    pub command_id: String,
    pub golden_path: String,
    pub golden_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisualScreen {
    #[serde(rename = "name")]
    pub screen: String,
    pub route: String,
    pub scenario: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy_route: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy_scenario: Option<String>,
    pub reference_path: String,
    pub reference_sha256: String,
    #[serde(rename = "width")]
    pub viewport_width: u32,
    #[serde(rename = "height")]
    pub viewport_height: u32,
    pub full_page: bool,
    pub mask_rectangles: Vec<MaskRectangle>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaskRectangle {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisualMatchParams {
    pub screens: Vec<VisualScreen>,
    pub tolerance_pct: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionSatisfiedParams {
    pub claim_id: String,
    pub decision_id: String,
    pub binding_basis: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaleClaimParams {
    pub claim_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SummedTotalsParams {
    pub claim_id: String,
    pub run_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmptyRunParams {
    pub claim_id: String,
    pub run_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DelegatedRunParams {
    pub claim_id: String,
    pub run_source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditDuringRunParams {
    pub run_id: String,
    pub edit_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CheckParams {
    TextAbsent(TextAbsentParams),
    SymbolAbsent(SymbolAbsentParams),
    UiCopyAbsent(UiCopyAbsentParams),
    PathPresent(PathPresentParams),
    TestPresentAndRuns(TestPresentAndRunsParams),
    FeatureDefaultPreserved(FeatureDefaultPreservedParams),
    EditsWithinPaths(EditsWithinPathsParams),
    PathsUntouched(PathsUntouchedParams),
    TermEverywhere(TermEverywhereParams),
    ForbiddenWords(ForbiddenWordsParams),
    CommandPasses(CommandPassesParams),
    FileEquals(FileEqualsParams),
    GoldenOutput(GoldenOutputParams),
    VisualMatch(VisualMatchParams),
    AssertionFloor(AssertionFloorParams),
    CiStepPresent(CiStepPresentParams),
    RuleFilePresent(RuleFilePresentParams),
    CommandForbidden(CommandForbiddenParams),
    TestsBeforeCode(TestsBeforeCodeParams),
    ConsentRecorded(ConsentRecordedParams),
    OneHeavyJob(OneHeavyJobParams),
    DecisionSatisfied(DecisionSatisfiedParams),
    StaleClaim(StaleClaimParams),
    SummedTotals(SummedTotalsParams),
    EmptyRun(EmptyRunParams),
    DelegatedRun(DelegatedRunParams),
    EditDuringRun(EditDuringRunParams),
    #[serde(rename = "deferred_decision")]
    Deferred(super::deferred::DeferredDecisionParams),
}

impl CheckParams {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::TextAbsent(_) => "text_absent",
            Self::SymbolAbsent(_) => "symbol_absent",
            Self::UiCopyAbsent(_) => "ui_copy_absent",
            Self::PathPresent(_) => "path_present",
            Self::TestPresentAndRuns(_) => "test_present_and_runs",
            Self::FeatureDefaultPreserved(_) => "feature_default_preserved",
            Self::EditsWithinPaths(_) => "edits_within_paths",
            Self::PathsUntouched(_) => "paths_untouched",
            Self::TermEverywhere(_) => "term_everywhere",
            Self::ForbiddenWords(_) => "forbidden_words",
            Self::CommandPasses(_) => "command_passes",
            Self::FileEquals(_) => "file_equals",
            Self::GoldenOutput(_) => "golden_output",
            Self::VisualMatch(_) => "visual_match",
            Self::AssertionFloor(_) => "assertion_floor",
            Self::CiStepPresent(_) => "ci_step_present",
            Self::RuleFilePresent(_) => "rule_file_present",
            Self::CommandForbidden(_) => "command_forbidden",
            Self::TestsBeforeCode(_) => "tests_before_code",
            Self::ConsentRecorded(_) => "consent_recorded",
            Self::OneHeavyJob(_) => "one_heavy_job",
            Self::DecisionSatisfied(_) => "decision_satisfied",
            Self::StaleClaim(_) => "stale_claim",
            Self::SummedTotals(_) => "summed_totals",
            Self::EmptyRun(_) => "empty_run",
            Self::DelegatedRun(_) => "delegated_run",
            Self::EditDuringRun(_) => "edit_during_run",
            Self::Deferred(_) => "deferred_decision",
        }
    }

    pub fn json(&self) -> Result<String> {
        Ok(serde_json::to_string(self)?)
    }

    fn validate(&self) -> Result<()> {
        match self {
            Self::TextAbsent(p) => {
                nonempty_list("terms", &p.terms)?;
                nonempty_list("surfaces", &p.surfaces)?;
                safe_allowlist(&p.allowlist)?;
                super::text_exclusions::validate_scopes(&p.test_allowlist)?;
                super::c31_json_field::validate_scopes(&p.json_field_allowlist)
                    .map_err(|reason| CoreError::Config(reason.into()))?;
                super::c31_archive::validate_scopes(&p.conditional_path_allowlist)
                    .map_err(|reason| CoreError::Config(reason.into()))?;
                super::c32_span_scope::validate_scopes(&p.span_allowlist, &p.test_allowlist)
                    .map_err(|reason| CoreError::Config(reason.into()))?;
            }
            Self::SymbolAbsent(p) => {
                nonempty("symbol", &p.symbol)?;
                nonempty_list("surfaces", &p.surfaces)?;
                safe_allowlist(&p.allowlist)?;
            }
            Self::UiCopyAbsent(p) => {
                nonempty_list("terms", &p.terms)?;
                safe_allowlist(&p.allowlist)?;
            }
            Self::PathPresent(p) => {
                relative_path("path", &p.path)?;
            }
            Self::TestPresentAndRuns(p) => {
                nonempty("name_pattern", &p.name_pattern)?;
                nonempty("command", &p.command)?;
            }
            Self::FeatureDefaultPreserved(p) => {
                relative_path("path", &p.path)?;
                nonempty("symbol", &p.symbol)?;
                nonempty("expected_value", &p.expected_value)?;
            }
            Self::EditsWithinPaths(p) => nonempty_list("allowed_paths", &p.allowed_paths)?,
            Self::PathsUntouched(p) => nonempty_list("paths", &p.paths)?,
            Self::TermEverywhere(p) => {
                nonempty("canonical", &p.canonical)?;
                nonempty_list("forbidden_variants", &p.forbidden_variants)?;
                nonempty_list("surfaces", &p.surfaces)?;
                safe_allowlist(&p.allowlist)?;
            }
            Self::ForbiddenWords(p) => {
                nonempty_list("words", &p.words)?;
                nonempty_list("surfaces", &p.surfaces)?;
                safe_allowlist(&p.allowlist)?;
            }
            Self::CommandPasses(p) => nonempty("command", &p.command)?,
            Self::FileEquals(p) => {
                relative_file("left", &p.left)?;
                relative_file("right", &p.right)?;
                if p.left == p.right {
                    return Err(CoreError::Config(
                        "file_equals requires two distinct paths".into(),
                    ));
                }
                if let Some(span) = &p.span {
                    if span.left.is_none() && span.right.is_none() {
                        return Err(CoreError::Config(
                            "file_equals span must select at least one byte range".into(),
                        ));
                    }
                    for selected in [&span.left, &span.right].into_iter().flatten() {
                        if selected.start >= selected.end {
                            return Err(CoreError::Config(
                                "file_equals byte span must be nonempty".into(),
                            ));
                        }
                    }
                }
            }
            Self::GoldenOutput(p) => {
                if !lower_sha256(&p.command_id) {
                    return Err(CoreError::Config(
                        "command_id must name a recorded binding digest".into(),
                    ));
                }
                relative_file("golden_path", &p.golden_path)?;
                if !lower_sha256(&p.golden_sha256) {
                    return Err(CoreError::Config(
                        "golden_sha256 must be a SHA-256 digest".into(),
                    ));
                }
            }
            Self::VisualMatch(p) => {
                if p.screens.is_empty()
                    || !p.tolerance_pct.is_finite()
                    || p.tolerance_pct < 0.0
                    || p.tolerance_pct > 100.0
                {
                    return Err(CoreError::Config("invalid visual_match parameters".into()));
                }
                for screen in &p.screens {
                    nonempty("screen", &screen.screen)?;
                    safe_visual_route(&screen.route)?;
                    if !matches!(
                        screen.scenario.as_str(),
                        "design" | "fixed-record" | "default" | "command_overlay"
                    ) {
                        return Err(CoreError::Config(
                            "scenario is not a closed visual scenario".into(),
                        ));
                    }
                    match (&screen.legacy_route, &screen.legacy_scenario) {
                        (Some(route), Some(scenario)) => {
                            safe_visual_route(route)?;
                            if !matches!(
                                scenario.as_str(),
                                "fixed-record" | "default" | "command_overlay"
                            ) {
                                return Err(CoreError::Config(
                                    "legacy_scenario is not a closed visual scenario".into(),
                                ));
                            }
                        }
                        (None, None) => {}
                        _ => {
                            return Err(CoreError::Config(
                                "legacy_route and legacy_scenario must be accepted together".into(),
                            ));
                        }
                    }
                    relative_path("reference_path", &screen.reference_path)?;
                    if screen.reference_sha256.len() != 64
                        || !screen
                            .reference_sha256
                            .bytes()
                            .all(|byte| byte.is_ascii_hexdigit())
                    {
                        return Err(CoreError::Config(
                            "reference_sha256 must be a SHA-256 hex digest".into(),
                        ));
                    }
                    if screen.viewport_width == 0 || screen.viewport_height == 0 {
                        return Err(CoreError::Config(
                            "visual screen width and height must be positive".into(),
                        ));
                    }
                    for rect in &screen.mask_rectangles {
                        let right = rect.x.checked_add(rect.width);
                        let bottom = rect.y.checked_add(rect.height);
                        if rect.width == 0
                            || rect.height == 0
                            || right.is_none_or(|edge| edge > screen.viewport_width)
                            || bottom.is_none()
                            || (!screen.full_page
                                && bottom.is_some_and(|edge| edge > screen.viewport_height))
                        {
                            return Err(CoreError::Config(
                                "mask rectangle lies outside the screen capture".into(),
                            ));
                        }
                    }
                }
            }
            Self::AssertionFloor(p) => {
                safe_allowlist(&p.paths)?;
                nonempty_list("paths", &p.paths)?;
                nonempty("baseline_version", &p.baseline_version)?;
                if p.minimum == 0 {
                    return Err(CoreError::Config("minimum must be positive".into()));
                }
            }
            Self::CiStepPresent(p) => {
                relative_file("workflow", &p.workflow)?;
                nonempty("step", &p.step)?;
                if p.threshold_pct.is_some_and(|threshold| threshold > 100) {
                    return Err(CoreError::Config("threshold_pct exceeds 100".into()));
                }
            }
            Self::RuleFilePresent(p) => {
                nonempty_list("paths", &p.paths)?;
                for path in &p.paths {
                    relative_file("rule file", path)?;
                    if path.contains(['*', '?', '[', ']']) {
                        return Err(CoreError::Config("rule files must name files".into()));
                    }
                }
                if p.person_edit_basis != "same_version" {
                    return Err(CoreError::Config("unsupported person_edit_basis".into()));
                }
            }
            Self::CommandForbidden(p) => {
                nonempty_text_list("patterns", &p.patterns)?;
                if let Some(scope) = &p.session_scope {
                    nonempty("session_scope", scope)?;
                }
            }
            Self::TestsBeforeCode(p) => {
                nonempty("change_id", &p.change_id)?;
                nonempty_list("test_paths", &p.test_paths)?;
                nonempty_list("source_paths", &p.source_paths)?;
                safe_allowlist(&p.test_paths)?;
                safe_allowlist(&p.source_paths)?;
            }
            Self::ConsentRecorded(p) => {
                nonempty("action_class", &p.action_class)?;
                nonempty("consent_event_kind", &p.consent_event_kind)?;
            }
            Self::OneHeavyJob(p) => {
                nonempty_text_list("command_patterns", &p.command_patterns)?;
                if p.window.start_ms >= p.window.end_ms {
                    return Err(CoreError::Config("heavy-job window is empty".into()));
                }
            }
            Self::DecisionSatisfied(p) => {
                nonempty("claim_id", &p.claim_id)?;
                nonempty("decision_id", &p.decision_id)?;
                if !matches!(
                    p.binding_basis.as_str(),
                    "explicit" | "accepted_term_overlap"
                ) {
                    return Err(CoreError::Config("unsupported binding_basis".into()));
                }
            }
            Self::StaleClaim(p) => nonempty("claim_id", &p.claim_id)?,
            Self::SummedTotals(p) => {
                nonempty("claim_id", &p.claim_id)?;
                nonempty_text_list("run_ids", &p.run_ids)?;
            }
            Self::EmptyRun(p) => {
                nonempty("claim_id", &p.claim_id)?;
                nonempty("run_id", &p.run_id)?;
            }
            Self::DelegatedRun(p) => {
                nonempty("claim_id", &p.claim_id)?;
                if !matches!(
                    p.run_source.as_str(),
                    "same_session" | "other_session" | "ci"
                ) {
                    return Err(CoreError::Config("unsupported run_source".into()));
                }
            }
            Self::EditDuringRun(p) => {
                nonempty("run_id", &p.run_id)?;
                nonempty_text_list("edit_ids", &p.edit_ids)?;
            }
            Self::Deferred(p) => p.validate()?,
        }
        Ok(())
    }
}

pub fn parse_params(kind: &str, schema_version: u32, raw_json: &str) -> Result<CheckParams> {
    if schema_version != PARAMS_SCHEMA_VERSION {
        return Err(CoreError::Config(format!(
            "unsupported decision params schema version {schema_version}"
        )));
    }
    let params: CheckParams = serde_json::from_str(raw_json)?;
    if params.kind() != kind {
        return Err(CoreError::Config(format!(
            "decision kind {kind} does not match params kind {}",
            params.kind()
        )));
    }
    params.validate()?;
    Ok(params)
}

fn nonempty(name: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        return Err(CoreError::Config(format!("{name} must be nonempty text")));
    }
    Ok(())
}

fn nonempty_list<T>(name: &str, values: &[T]) -> Result<()> {
    if values.is_empty() {
        return Err(CoreError::Config(format!("{name} must not be empty")));
    }
    Ok(())
}

fn nonempty_text_list(name: &str, values: &[String]) -> Result<()> {
    nonempty_list(name, values)?;
    for value in values {
        nonempty(name, value)?;
    }
    Ok(())
}

fn safe_allowlist(paths: &[String]) -> Result<()> {
    for path in paths {
        relative_path("allowlist path", path)?;
    }
    Ok(())
}

fn lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn relative_file(name: &str, path: &str) -> Result<()> {
    relative_path(name, path)?;
    if path.split('/').any(|part| part.is_empty() || part == ".") {
        return Err(CoreError::Config(format!(
            "{name} must name a project file"
        )));
    }
    Ok(())
}

fn safe_visual_route(route: &str) -> Result<()> {
    nonempty("route", route)?;
    if !(route.starts_with("#/") || route.starts_with('/'))
        || route.starts_with("//")
        || route.contains("://")
        || route.contains("..")
        || route.contains('\\')
    {
        return Err(CoreError::Config(
            "visual route must be a local path or hash route".into(),
        ));
    }
    Ok(())
}

fn relative_path(name: &str, path: &str) -> Result<()> {
    nonempty(name, path)?;
    if path.starts_with('/')
        || path.starts_with('~')
        || path.split('/').any(|part| part == "..")
        || path.contains('\\')
    {
        return Err(CoreError::Config(format!(
            "{name} must be a relative project path"
        )));
    }
    Ok(())
}
