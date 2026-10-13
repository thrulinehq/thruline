//! C1: measure a pinned screen capture against a person-pinned PNG.
//!
//! The comparison is a fixed pixel census. Capture receipts carry the exact
//! Git tree and renderer identity; a missing or mismatched receipt never holds.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::redact::Masked;

use super::checks::{CheckOutcome, Finding, SurfaceResult, VerdictState};
use super::params::{Surface, VisualMatchParams, VisualScreen};

#[path = "visual_ui_build.rs"]
pub mod visual_build;
#[path = "visual_build.rs"]
mod visual_build_legacy;
#[path = "visual_cache.rs"]
pub mod visual_cache;
#[path = "visual_cached.rs"]
mod visual_cached;
#[path = "visual_cargo_target.rs"]
mod visual_cargo_target;
#[path = "visual_history.rs"]
pub mod visual_history;
#[path = "visual_identity.rs"]
pub mod visual_identity;
#[path = "visual_inputs.rs"]
pub mod visual_inputs;
#[path = "visual_process.rs"]
pub mod visual_process;
#[path = "visual_profile.rs"]
mod visual_profile;
#[path = "visual_progress.rs"]
mod visual_progress;
#[path = "visual_record.rs"]
pub mod visual_record;
#[path = "visual_snapshot.rs"]
pub mod visual_snapshot;
#[path = "visual_static.rs"]
pub mod visual_static;
use visual_inputs::valid_utc_second;
use visual_profile::VisualProfile;
pub use visual_progress::{with_visual_progress_observer, VisualProgress, VisualProgressObserver};

const COMPARE_SCRIPT: &str = include_str!("visual_compare.py");
const CAPTURE_SCRIPT: &str = include_str!("visual_capture.mjs");
const VISUAL_IMPL_VERSION: &str = "c1-pixel-v9-pinned-apfs-snapshot";
/// C28's approved local renderer. A caller-provided digest cannot approve a
/// different executable; changing this pin requires a new person instruction.
/// Existing C28-approved host installation, strengthened to pin the framework
/// behind the approved launcher; a changed same-product framework must abstain.
pub const APPROVED_RENDERER_FRAMEWORK_SHA256: &str =
    "20fde9bb98dd5f5fb4b379da45c8f92fcb4f669e923c7e60a14f8e4329aa2bf9";
pub const APPROVED_RENDERER_PRODUCT: &str = "Chrome/154.0.8037.98";
pub const APPROVED_RENDERER_SHA256: &str =
    "ccffd5c5fe77ba3212e07c2c69091f8048b73c396b4647ecaa150e1e01a04954";
// The historical CLI exports the complete scratch ledger before printing its
// ready URL. The founder's 8.6 GiB record can take longer than five minutes.
const EXACT_SERVER_START_TIMEOUT: Duration = Duration::from_secs(30 * 60);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScrollRegion {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub content_width: u32,
    pub content_height: u32,
    pub overflow_x: bool,
    pub overflow_y: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HousekeepingResponse {
    pub path: String,
    pub status: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureReceipt {
    pub screen: String,
    /// Accepted primary address and scenario, retained for input binding.
    pub route: String,
    pub scenario: String,
    #[serde(default)]
    pub actual_route: Option<String>,
    #[serde(default)]
    pub actual_scenario: Option<String>,
    pub commit_oid: String,
    pub tree_hash: String,
    pub renderer_sha256: String,
    #[serde(default)]
    pub renderer_version: Option<String>,
    #[serde(default)]
    pub capture_clock: Option<String>,
    #[serde(default)]
    pub fixture_sha256: Option<String>,
    #[serde(default)]
    pub frame_width: Option<u32>,
    #[serde(default)]
    pub frame_height: Option<u32>,
    #[serde(default)]
    pub content_width: Option<u32>,
    #[serde(default)]
    pub content_height: Option<u32>,
    #[serde(default)]
    pub overflow_x: Option<bool>,
    #[serde(default)]
    pub overflow_y: Option<bool>,
    #[serde(default)]
    pub scroll_regions: Vec<ScrollRegion>,
    #[serde(default)]
    pub clipped_regions: Vec<ScrollRegion>,
    #[serde(default)]
    pub housekeeping_responses: Vec<HousekeepingResponse>,
    pub route_found: bool,
    pub screenshot_path: Option<PathBuf>,
    pub failure: Option<String>,
}

#[derive(Debug, Clone)]
pub struct VisualRuntime {
    pub reference_root: PathBuf,
    /// SHA-256 of the renderer executable verified by the invoking adapter.
    pub renderer_sha256: Option<String>,
    pub renderer_version: Option<String>,
    pub capture_clock: Option<String>,
    pub fixture_sha256: Option<String>,
    pub captures: Vec<CaptureReceipt>,
    pub diff_root: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VisualScreenResult {
    pub screen: String,
    pub route: String,
    pub actual_route: Option<String>,
    pub actual_scenario: Option<String>,
    pub state: VerdictState,
    pub difference_pct: Option<f64>,
    pub different_pixels: Option<u64>,
    pub checked_pixels: Option<u64>,
    pub reference_width: Option<u32>,
    pub reference_height: Option<u32>,
    pub capture_width: Option<u32>,
    pub capture_height: Option<u32>,
    pub compared_width: Option<u32>,
    pub compared_height: Option<u32>,
    pub diff_image_path: Option<String>,
    pub diff_sha256: Option<String>,
    pub reference_sha256: String,
    pub version_id: Option<String>,
    pub commit_oid: Option<String>,
    pub tree_hash: Option<String>,
    pub renderer_sha256: Option<String>,
    #[serde(default)]
    pub renderer_version: Option<String>,
    pub capture_clock: Option<String>,
    #[serde(default)]
    pub fixture_sha256: Option<String>,
    #[serde(default)]
    pub frame_width: Option<u32>,
    #[serde(default)]
    pub frame_height: Option<u32>,
    #[serde(default)]
    pub content_width: Option<u32>,
    #[serde(default)]
    pub content_height: Option<u32>,
    #[serde(default)]
    pub overflow_x: Option<bool>,
    #[serde(default)]
    pub overflow_y: Option<bool>,
    #[serde(default)]
    pub scroll_regions: Vec<ScrollRegion>,
    #[serde(default)]
    pub clipped_regions: Vec<ScrollRegion>,
    #[serde(default)]
    pub housekeeping_responses: Vec<HousekeepingResponse>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VisualReport {
    pub outcome: CheckOutcome,
    pub screens: Vec<VisualScreenResult>,
}

/// Stable tool identity used by the engine's cache and replay basis. An absent
/// machine pin stays explicit and will be Unverifiable in `check_visual_at`.
pub fn visual_tool_identity(params: &VisualMatchParams) -> Value {
    let renderer = std::env::var_os("THRULINE_VISUAL_RENDERER").map(PathBuf::from);
    let expected = std::env::var("THRULINE_VISUAL_RENDERER_SHA256").ok();
    let verified = renderer
        .as_deref()
        .zip(expected.as_deref())
        .and_then(|(renderer, pin)| {
            let cache = visual_cache::VisualCache::from_env()?;
            visual_identity::verify_renderer_with(
                renderer,
                pin,
                APPROVED_RENDERER_SHA256,
                APPROVED_RENDERER_PRODUCT,
                &cache,
            )
            .ok()
        });
    let actual = verified.as_ref().map(|v| v.sha256.clone());
    let version = verified.as_ref().map(|v| {
        Masked::mask(&v.product)
            .into_text()
            .chars()
            .take(120)
            .collect::<String>()
    });
    // The original caller pin remains the verifier input above. This is the
    // outbound basis copy, so an invalid env value cannot expose a credential.
    let expected_display = expected.as_deref().map(|pin| {
        Masked::mask(pin)
            .into_text()
            .chars()
            .take(120)
            .collect::<String>()
    });
    let python = std::env::var_os("THRULINE_VISUAL_PYTHON").unwrap_or_else(|| "python3".into());
    let diff_root = std::env::var_os("THRULINE_VISUAL_DIFF_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("thruline-c1-diffs"));
    let caches = build_caches();
    let python_userbase = caches
        .as_ref()
        .and_then(|cache| python_userbase(&cache.account_home));
    json!({
        "visual_impl": VISUAL_IMPL_VERSION,
        "visual_compare_sha256": digest(COMPARE_SCRIPT.as_bytes()),
        "visual_capture_sha256": digest(CAPTURE_SCRIPT.as_bytes()),
        "node_version": tool_version("node", "--version"),
        "npm_version": tool_version("npm", "--version"),
        "cargo_version": caches.as_ref().and_then(cargo_version),
        "python_version": tool_version(&python, "--version"),
        "diff_root_sha256": digest(diff_root.to_string_lossy().as_bytes()),
        "offline_cargo_cache_sha256": caches.as_ref().map(|cache| digest(cache.cargo_home.to_string_lossy().as_bytes())),
        "offline_npm_cache_sha256": caches.as_ref().map(|cache| digest(cache.npm_cache.to_string_lossy().as_bytes())),
        "python_userbase_sha256": python_userbase.as_ref().map(|base| digest(base.to_string_lossy().as_bytes())),
        "build_flags": {
            "cargo": "--offline --release --no-default-features -p thruline-cli --bin thruline",
            "cargo_jobs": 2,
            "cargo_debug": "0",
            "cargo_incremental": "0",
            "npm_install": "ci --offline --no-audit --no-fund",
            "ui_generated_output": if std::env::var_os("THRULINE_VISUAL_CARGO_TARGET_ROOT").is_some() { "pinned-external-v1" } else { "internal-source-legacy" },
            "ui_output_root_sha256": std::env::var_os("THRULINE_VISUAL_CARGO_TARGET_ROOT").map(|root| digest(root.to_string_lossy().as_bytes())),
            "record_snapshot": "internal-apfs-exact-bytes-v1",
            "native_upgrade_reserve": "15-gib-plus-complete-snapshot-bytes",
            "design": params.screens.iter().any(|screen| screen.scenario == "design"),
            "export_clock": "explicit frozen record manifest, UTC second",
            "frame": "verified accepted PNG IHDR; no full-page enlargement or masks",
        },
        "renderer_sha256": actual,
        "renderer_pin_sha256": expected_display,
        "approved_renderer_sha256": APPROVED_RENDERER_SHA256,
        "approved_renderer_framework_sha256": APPROVED_RENDERER_FRAMEWORK_SHA256,
        "renderer_version": version,
    })
}

fn cargo_version(caches: &BuildCaches) -> Option<String> {
    let output = Command::new("cargo")
        .arg("--version")
        .env("CARGO_HOME", &caches.cargo_home)
        .env("RUSTUP_HOME", &caches.rustup_home)
        .env("RUSTUP_AUTO_INSTALL", "0")
        .output()
        .ok()?;
    output.status.success().then(|| {
        Masked::mask(String::from_utf8_lossy(&output.stdout).trim())
            .into_text()
            .chars()
            .take(120)
            .collect()
    })
}

fn tool_version(program: impl AsRef<std::ffi::OsStr>, flag: &str) -> Option<String> {
    let output = Command::new(program).arg(flag).output().ok()?;
    output.status.success().then(|| {
        Masked::mask(String::from_utf8_lossy(&output.stdout).trim())
            .into_text()
            .chars()
            .take(120)
            .collect::<String>()
    })
}

/// Production entry point. It builds the named UI and server from the exact
/// Git commit, then captures local routes on the caller's scratch ledger.
pub fn check_visual_at(
    repo: &Path,
    version_id: &str,
    params: &VisualMatchParams,
    db_path: &Path,
) -> CheckOutcome {
    check_visual_at_for_decision(
        repo,
        version_id,
        params,
        db_path,
        &format!("diagnostic/{}", visual_cache::key(params)),
        0,
    )
}

/// Accepted-decision adapter. Revision joins every per-screen result key.
pub fn check_visual_at_for_decision(
    repo: &Path,
    version_id: &str,
    params: &VisualMatchParams,
    db_path: &Path,
    decision_id: &str,
    revision: i64,
) -> CheckOutcome {
    let profile = VisualProfile::from_env();
    visual_progress::emit(
        0,
        params.screens.len(),
        format!(
            "0 of {} screens · checking frozen inputs",
            params.screens.len()
        ),
    );
    if params
        .screens
        .iter()
        .any(|screen| !screen.mask_rectangles.is_empty())
    {
        return unavailable_report(params, "C1 reference frames do not permit masks", None).outcome;
    }
    if params.screens.is_empty() || params.screens.len() > 64 || {
        let mut names = std::collections::BTreeSet::new();
        params
            .screens
            .iter()
            .any(|screen| !names.insert(&screen.screen))
    } {
        return unavailable_report(
            params,
            "visual screen names must be unique and bounded",
            None,
        )
        .outcome;
    }
    let Some(cache) = visual_cache::VisualCache::from_env() else {
        return unavailable_report(params, "private visual cache is unavailable", None).outcome;
    };
    let frozen = {
        let _stage = profile.span("fixture_verify");
        match visual_inputs::load_cached_fixture_manifest_at(
            db_path,
            &visual_inputs::manifest_path(db_path),
            &cache,
        ) {
            Ok(frozen) => frozen,
            Err(reason) => return unavailable_report(params, reason, None).outcome,
        }
    };
    let capture_clock = frozen.manifest.capture_clock;
    let fixture_sha256 = frozen.manifest_sha256;
    let record_sha256 = frozen.manifest.record_sha256.to_ascii_lowercase();
    let Some(renderer) = std::env::var_os("THRULINE_VISUAL_RENDERER").map(PathBuf::from) else {
        return unavailable_report(params, "renderer not found", None).outcome;
    };
    let Some(pin) = std::env::var("THRULINE_VISUAL_RENDERER_SHA256").ok() else {
        return unavailable_report(params, "renderer is not pinned", None).outcome;
    };
    let renderer_stage = profile.span("renderer_verify");
    let verified = match visual_identity::verify_renderer_with(
        &renderer,
        &pin,
        APPROVED_RENDERER_SHA256,
        APPROVED_RENDERER_PRODUCT,
        &cache,
    ) {
        Ok(v) => v,
        Err(reason) => return unavailable_report(params, reason, None).outcome,
    };
    let actual = verified.sha256;
    drop(renderer_stage);
    let Some(commit_oid) = git_oid(repo, &format!("{version_id}^{{commit}}")) else {
        return unavailable_report(params, "version cannot be resolved to a commit", None).outcome;
    };
    let Some(tree_hash) = git_oid(repo, &format!("{commit_oid}^{{tree}}")) else {
        return unavailable_report(params, "version tree cannot be resolved", None).outcome;
    };
    let Ok(diff_root) = prepare_diff_root() else {
        return unavailable_report(
            params,
            "private diff storage is unavailable",
            Some(tree_hash),
        )
        .outcome;
    };
    let Ok(work) = tempfile::tempdir() else {
        return unavailable_report(
            params,
            "visual scratch space is unavailable",
            Some(tree_hash),
        )
        .outcome;
    };
    let capture_context = CaptureContext {
        repo,
        commit_oid: &commit_oid,
        tree_hash: &tree_hash,
        params,
        db_path: &frozen.record_path,
        renderer: &renderer,
        renderer_sha256: &actual,
        capture_clock: &capture_clock,
        fixture_sha256: &fixture_sha256,
        record_stamp: &frozen.record_stamp,
        record_sha256: &record_sha256,
        witness_event_id: frozen
            .manifest
            .clock_witness
            .as_ref()
            .map(|witness| witness.event_id.as_str()),
        profile: &profile,
        work: work.path(),
    };
    let reference_root = std::env::var_os("THRULINE_VISUAL_REFERENCE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo.to_path_buf());
    if visual_cached::all_primary_closed(repo, &commit_oid, params) {
        return visual_cached::check_closed(
            capture_context,
            &cache,
            visual_cached::DecisionContext {
                id: decision_id,
                revision,
            },
            &reference_root,
            &diff_root,
        );
    }
    let history_input = visual_history::HistoricalInput {
        implementation: VISUAL_IMPL_VERSION.into(),
        commit_oid: commit_oid.clone(),
        tree_hash: tree_hash.clone(),
        capture_tool_sha256: digest(CAPTURE_SCRIPT.as_bytes()),
        fixture_sha256: fixture_sha256.clone(),
        renderer_sha256: actual.clone(),
        renderer_framework_sha256: APPROVED_RENDERER_FRAMEWORK_SHA256.into(),
        renderer_version: APPROVED_RENDERER_PRODUCT.into(),
        capture_clock: capture_clock.clone(),
        screens: params
            .screens
            .iter()
            .map(|screen| {
                let (width, height) = reference_frame(repo, screen).unwrap_or((0, 0));
                visual_history::HistoricalScreen {
                    screen: screen.screen.clone(),
                    route: screen.route.clone(),
                    scenario: screen.scenario.clone(),
                    legacy_route: screen.legacy_route.clone(),
                    legacy_scenario: screen.legacy_scenario.clone(),
                    width,
                    height,
                }
            })
            .collect(),
    };
    let mut history_build = None;
    let captures = if let Some(captures) = visual_history::load_captures(&cache, &history_input) {
        for (i, _) in captures.iter().enumerate() {
            visual_progress::emit(
                i + 1,
                params.screens.len(),
                format!(
                    "{} of {} screens · cached historical capture",
                    i + 1,
                    params.screens.len()
                ),
            );
        }
        captures
    } else {
        capture_exact_version(capture_context, &cache, &mut history_build)
    };
    let decision = visual_cached::DecisionContext {
        id: decision_id,
        revision,
    };
    let outcome = check_historical_results(
        &capture_context,
        &cache,
        &history_input,
        decision,
        &reference_root,
        &diff_root,
        &captures,
    );
    if outcome
        .visual
        .iter()
        .all(|screen| screen.state != VerdictState::Unverifiable)
    {
        if let Some(build) = history_build {
            let _ = visual_history::save_captures(&cache, &history_input, &build, &captures);
        }
        save_historical_results(
            &cache,
            &history_input,
            decision,
            params,
            &reference_root,
            &outcome.visual,
        );
    }
    outcome
}

fn historical_result_key(
    capture: &str,
    decision: visual_cached::DecisionContext<'_>,
    screen: &VisualScreen,
    tolerance_pct: f64,
) -> visual_cache::ResultKey {
    visual_cache::ResultKey {
        capture_key: capture.into(),
        decision_id: decision.id.into(),
        revision: decision.revision,
        reference_sha256: screen.reference_sha256.clone(),
        tolerance_bits: tolerance_pct.to_bits(),
        compare_tool_sha256: digest(COMPARE_SCRIPT.as_bytes()),
    }
}

fn check_historical_results(
    context: &CaptureContext<'_>,
    cache: &visual_cache::VisualCache,
    input: &visual_history::HistoricalInput,
    decision: visual_cached::DecisionContext<'_>,
    reference_root: &Path,
    diff_root: &Path,
    captures: &[CaptureReceipt],
) -> CheckOutcome {
    let keys = visual_history::bound_capture_keys(cache, input);
    let check = ScreenCheckContext {
        reference_root,
        diff_root: Some(diff_root),
        commit_oid: context.commit_oid,
        tree_hash: context.tree_hash,
        renderer_pin: context.renderer_sha256,
        renderer_version: Some(APPROVED_RENDERER_PRODUCT),
        tolerance_pct: context.params.tolerance_pct,
        capture_clock: Some(context.capture_clock),
        fixture_sha256: Some(context.fixture_sha256),
    };
    let screens = context
        .params
        .screens
        .iter()
        .enumerate()
        .map(|(i, screen)| {
            let result = keys
                .as_ref()
                .and_then(|keys| keys.get(i))
                .and_then(|capture_key| {
                    let key = historical_result_key(
                        capture_key,
                        decision,
                        screen,
                        context.params.tolerance_pct,
                    );
                    cache
                        .load::<VisualScreenResult>("history-result", &key.key())
                        .or_else(|| cache.load("history-comparison", &key.comparison_key()))
                        .filter(|r| {
                            r.state != VerdictState::Unverifiable
                                && r.screen == screen.screen
                                && r.route == screen.route
                                && r.reference_sha256 == screen.reference_sha256
                                && r.commit_oid.as_deref() == Some(context.commit_oid)
                                && r.tree_hash.as_deref() == Some(context.tree_hash)
                        })
                })
                .unwrap_or_else(|| check_screen(screen, captures.get(i), &check));
            visual_progress::emit(
                i + 1,
                context.params.screens.len(),
                format!(
                    "{} of {} screens · historical comparison",
                    i + 1,
                    context.params.screens.len()
                ),
            );
            result
        })
        .collect();
    report_from_screens(screens, context.tree_hash.into()).outcome
}

fn save_historical_results(
    cache: &visual_cache::VisualCache,
    input: &visual_history::HistoricalInput,
    decision: visual_cached::DecisionContext<'_>,
    params: &VisualMatchParams,
    reference_root: &Path,
    results: &[VisualScreenResult],
) {
    let Some(keys) = visual_history::bound_capture_keys(cache, input) else {
        return;
    };
    let Some(saved) = visual_history::load_captures(cache, input) else {
        return;
    };
    for (i, result) in results.iter().enumerate() {
        let Some(screen) = params.screens.get(i) else {
            return;
        };
        let Some(key) = keys.get(i) else {
            return;
        };
        let key = historical_result_key(key, decision, screen, params.tolerance_pct);
        let mut files = vec![reference_root.join(&screen.reference_path)];
        // First-run screenshots live in disposable work; use the preserved
        // verified copy named by the historical capture cache.
        if let Some(png) = saved.get(i).and_then(|c| c.screenshot_path.as_ref()) {
            files.push(png.clone());
        }
        if let Some(diff) = &result.diff_image_path {
            files.push(PathBuf::from(diff));
        }
        let _ = cache.save("history-comparison", &key.comparison_key(), result, &files);
        let _ = cache.save("history-result", &key.key(), result, &files);
    }
}

fn prepare_diff_root() -> Result<PathBuf, ()> {
    let root = std::env::var_os("THRULINE_VISUAL_DIFF_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("thruline-c1-diffs"));
    if !root.is_absolute()
        || std::fs::symlink_metadata(&root).is_ok_and(|meta| meta.file_type().is_symlink())
    {
        return Err(());
    }
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&root).map_err(|_| ())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if std::fs::metadata(&root)
            .map_err(|_| ())?
            .permissions()
            .mode()
            & 0o077
            != 0
        {
            return Err(());
        }
    }
    Ok(root)
}

#[derive(Debug, Serialize)]
struct CaptureJob<'a> {
    screen: &'a str,
    route: &'a str,
    scenario: &'a str,
    width: u32,
    height: u32,
    full_page: bool,
    output: PathBuf,
}

#[derive(Debug, Deserialize)]
struct CapturedShot {
    screen: String,
    screenshot_path: Option<PathBuf>,
    error: Option<String>,
    capture_clock: Option<String>,
    fixture_sha256: Option<String>,
    renderer_version: Option<String>,
    frame_width: Option<u32>,
    frame_height: Option<u32>,
    content_width: Option<u32>,
    content_height: Option<u32>,
    overflow_x: Option<bool>,
    overflow_y: Option<bool>,
    #[serde(default)]
    scroll_regions: Vec<ScrollRegion>,
    #[serde(default)]
    clipped_regions: Vec<ScrollRegion>,
    #[serde(default)]
    housekeeping_responses: Vec<HousekeepingResponse>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedScreenRoute {
    pub route: String,
    pub scenario: String,
}

/// The primary address always wins. A fallback address is used only when the
/// accepted primary is absent from the exact version's source router.
pub fn resolve_screen_route(
    repo: &Path,
    version_id: &str,
    screen: &VisualScreen,
) -> Option<ResolvedScreenRoute> {
    if route_in_version(repo, version_id, &screen.route)? {
        let selected = ResolvedScreenRoute {
            route: screen.route.clone(),
            scenario: screen.scenario.clone(),
        };
        return scenario_in_version(repo, version_id, &selected).then_some(selected);
    }
    let legacy_route = screen.legacy_route.as_deref()?;
    if !route_in_version(repo, version_id, legacy_route)? {
        return None;
    }
    let selected = ResolvedScreenRoute {
        route: legacy_route.into(),
        scenario: screen.legacy_scenario.clone()?,
    };
    scenario_in_version(repo, version_id, &selected).then_some(selected)
}

fn scenario_in_version(repo: &Path, version_id: &str, selected: &ResolvedScreenRoute) -> bool {
    let Some(commit_oid) = git_oid(repo, &format!("{version_id}^{{commit}}")) else {
        return false;
    };
    match selected.scenario.as_str() {
        "design" => is_design_route(&selected.route),
        "fixed-record" | "default" => !is_design_route(&selected.route),
        "command_overlay" => {
            git_text(repo, &format!("{commit_oid}:ui/src/hq/app.jsx"))
                .is_some_and(|source| source.contains("Command-Ask"))
                || git_text(repo, &format!("{commit_oid}:ui/src/workspace/chrome.jsx"))
                    .is_some_and(|source| source.contains("aria-label=\"Command palette\""))
        }
        _ => false,
    }
}

fn is_design_route(route: &str) -> bool {
    route.starts_with("#/design/") || route.starts_with("/#/design/")
}

/// The UI check deliberately uses a scratch HOME, but the exact historical
/// build still needs the machine's already-installed, offline package caches.
/// Find them from the installed Cargo shim rather than from HOME, which may be
/// an empty scratch directory. A nonstandard installation can set CARGO_HOME.
struct BuildCaches {
    account_home: PathBuf,
    cargo_home: PathBuf,
    rustup_home: PathBuf,
    npm_cache: PathBuf,
}

fn build_caches() -> Option<BuildCaches> {
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            let path = std::env::var_os("PATH")?;
            std::env::split_paths(&path)
                .map(|part| part.join("cargo"))
                .find(|candidate| candidate.is_file())
                .and_then(|cargo| {
                    let bin = cargo.parent()?;
                    (bin.file_name()? == "bin")
                        .then(|| bin.parent())
                        .flatten()
                        .filter(|home| home.file_name().is_some_and(|name| name == ".cargo"))
                        .map(Path::to_path_buf)
                })
        })?;
    let account_home = cargo_home.parent()?;
    let rustup_home = std::env::var_os("RUSTUP_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| account_home.join(".rustup"));
    let npm_cache = std::env::var_os("npm_config_cache")
        .map(PathBuf::from)
        .unwrap_or_else(|| account_home.join(".npm"));
    (cargo_home.is_dir() && rustup_home.is_dir() && npm_cache.is_dir()).then_some(BuildCaches {
        account_home: account_home.to_path_buf(),
        cargo_home,
        rustup_home,
        npm_cache,
    })
}

fn python_userbase(account_home: &Path) -> Option<PathBuf> {
    let python = std::env::var_os("THRULINE_VISUAL_PYTHON").unwrap_or_else(|| "python3".into());
    let output = Command::new(python)
        .args(["-c", "import site; print(site.getuserbase())"])
        .env("HOME", account_home)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = PathBuf::from(String::from_utf8(output.stdout).ok()?.trim());
    (path.is_absolute() && path.is_dir()).then_some(path)
}

#[derive(Clone, Copy)]
struct CaptureContext<'a> {
    repo: &'a Path,
    commit_oid: &'a str,
    tree_hash: &'a str,
    params: &'a VisualMatchParams,
    db_path: &'a Path,
    renderer: &'a Path,
    renderer_sha256: &'a str,
    capture_clock: &'a str,
    fixture_sha256: &'a str,
    record_stamp: &'a visual_inputs::FileStamp,
    record_sha256: &'a str,
    witness_event_id: Option<&'a str>,
    profile: &'a VisualProfile,
    work: &'a Path,
}

fn capture_exact_version(
    context: CaptureContext<'_>,
    cache: &visual_cache::VisualCache,
    preserved_build: &mut Option<visual_history::HistoricalBuild>,
) -> Vec<CaptureReceipt> {
    let CaptureContext {
        repo,
        commit_oid,
        tree_hash,
        params,
        db_path,
        renderer,
        renderer_sha256,
        capture_clock,
        fixture_sha256,
        record_stamp,
        record_sha256,
        witness_event_id,
        profile,
        work,
    } = context;
    let routes_stage = profile.span("resolve_routes_and_frames");
    let routes: Vec<_> = params
        .screens
        .iter()
        .map(|screen| resolve_screen_route(repo, commit_oid, screen))
        .collect();
    drop(routes_stage);
    let mut receipts: Vec<_> = params
        .screens
        .iter()
        .zip(routes.iter())
        .map(|(screen, found)| {
            let frame = reference_frame(repo, screen);
            let primary = route_in_version(repo, commit_oid, &screen.route);
            let alias_unknown = primary == Some(false)
                && screen
                    .legacy_route
                    .as_deref()
                    .is_some_and(|alias| route_in_version(repo, commit_oid, alias).is_none());
            let route_unknown = primary.is_none() || alias_unknown;
            CaptureReceipt {
                screen: screen.screen.clone(),
                route: screen.route.clone(),
                scenario: screen.scenario.clone(),
                commit_oid: commit_oid.into(),
                tree_hash: tree_hash.into(),
                renderer_sha256: renderer_sha256.into(),
                renderer_version: None,
                capture_clock: Some(capture_clock.into()),
                fixture_sha256: Some(fixture_sha256.into()),
                frame_width: frame.map(|frame| frame.0),
                frame_height: frame.map(|frame| frame.1),
                content_width: None,
                content_height: None,
                overflow_x: None,
                overflow_y: None,
                scroll_regions: Vec::new(),
                clipped_regions: Vec::new(),
                housekeeping_responses: Vec::new(),
                route_found: found.is_some() || route_unknown,
                actual_route: found.as_ref().map(|route| route.route.clone()),
                actual_scenario: found.as_ref().map(|route| route.scenario.clone()),
                screenshot_path: None,
                failure: if route_unknown {
                    Some("exact route table is unavailable".into())
                } else if found.is_some() && frame.is_none() {
                    Some(
                        "accepted reference PNG frame is unavailable or does not match its pin"
                            .into(),
                    )
                } else {
                    None
                },
            }
        })
        .collect();
    if !receipts
        .iter()
        .any(|receipt| receipt.route_found && receipt.failure.is_none())
    {
        return receipts;
    }
    let built = (|| -> Result<(PathBuf, PathBuf), &'static str> {
        let mut target = visual_cargo_target::CargoTarget::from_env(commit_oid)?;
        target.prepare()?;
        let caches = build_caches().ok_or("offline build caches are unavailable")?;
        let source = work.join("source");
        std::fs::create_dir_all(&source).map_err(|_| "visual scratch space is unavailable")?;
        let archive = work.join("source.tar");
        let archive_stage = profile.span("archive");
        let archive_status = crate::git_isolation::isolate_config(&mut Command::new("git"))
            .args(["archive", "--format=tar", "--output"])
            .arg(&archive)
            .arg(commit_oid)
            .current_dir(repo)
            .status()
            .map_err(|_| "exact source archive is unavailable")?;
        if !archive_status.success() {
            return Err("exact source archive failed");
        }
        let unpack = Command::new("tar")
            .arg("-xf")
            .arg(&archive)
            .arg("-C")
            .arg(&source)
            .status()
            .map_err(|_| "exact source unpack is unavailable")?;
        if !unpack.success() {
            return Err("exact source unpack failed");
        }
        drop(archive_stage);
        let ui = source.join("ui");
        if !ui.join("package-lock.json").is_file() {
            return Err("exact version has no pinned UI dependencies");
        }
        let design_build = routes
            .iter()
            .flatten()
            .any(|route| route.scenario == "design" && is_design_route(&route.route));
        let external_ui = if std::env::var_os("THRULINE_VISUAL_CARGO_TARGET_ROOT").is_some() {
            Some(visual_build::build_external_ui(
                repo,
                &ui,
                &target,
                &caches.npm_cache,
                profile,
                design_build,
            )?)
        } else {
            let npm_stage = profile.span("npm_install");
            let npm = Command::new("npm")
                .args(["ci", "--offline", "--no-audit", "--no-fund"])
                .env("npm_config_cache", &caches.npm_cache)
                .current_dir(&ui)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map_err(|_| "UI dependency installer is unavailable")?;
            if !npm.success() {
                return Err("exact UI dependencies are unavailable offline");
            }
            drop(npm_stage);
            let build_stage = profile.span("npm_build");
            let build = Command::new("npm")
                .args(["run", "build"])
                .env("VITE_HQ_DESIGN", if design_build { "1" } else { "0" })
                .current_dir(&ui)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map_err(|_| "exact UI builder is unavailable")?;
            if !build.success() || !ui.join("dist/index.html").is_file() {
                return Err("exact UI build failed");
            }
            drop(build_stage);
            None
        };
        // Cargo fingerprints are not safe to share between archived workspaces
        // that reuse package names and versions while changing their sources.
        // In particular, a later licence crate can mask the archived module.
        let cargo_stage = profile.span("cargo_build");
        let mut command = Command::new("cargo");
        command
            .args([
                "build",
                "--offline",
                "--release",
                "--no-default-features",
                "-p",
                "thruline-cli",
                "--bin",
                "thruline",
            ])
            .env("CARGO_HOME", &caches.cargo_home)
            .env("RUSTUP_HOME", &caches.rustup_home)
            .env("RUSTUP_AUTO_INSTALL", "0")
            .current_dir(&source);
        let cargo = match &external_ui {
            Some(workspace) => {
                target.run_with_checks(&mut command, || workspace.verify_staging())?
            }
            None => target.run(&mut command)?,
        };
        if let Some(workspace) = &external_ui {
            workspace.verify()?;
        }

        let server = target.target.join("release/thruline");
        if !cargo.status.success() || !server.is_file() {
            write_private_build_diagnostic(&cargo);
            let stderr = String::from_utf8_lossy(&cargo.stderr);
            let reason = if stderr.contains("no matching package named") {
                "exact server build failed: offline Cargo cache lacks a pinned dependency"
            } else if stderr.contains("toolchain") && stderr.contains("not installed") {
                "exact server build failed: pinned Rust toolchain is unavailable offline"
            } else {
                "exact server build failed: historical source did not compile"
            };
            return Err(reason);
        }
        drop(cargo_stage);
        Ok((
            external_ui
                .as_ref()
                .map_or_else(|| ui.join("dist"), |workspace| workspace.dist.clone()),
            server,
        ))
    })();
    let (dist, server_bin) = match built {
        Ok(built) => built,
        Err(reason) => {
            for receipt in &mut receipts {
                if receipt.route_found {
                    receipt.failure = Some(reason.into());
                }
            }
            return receipts;
        }
    };
    let snapshot = work.join("ledger.db");
    let snapshot_stage = profile.span("snapshot");
    let snapshot_result = if visual_inputs::file_stamp(db_path).as_ref() != Some(record_stamp) {
        Err("frozen record changed before its capture snapshot")
    } else {
        visual_snapshot::snapshot_frozen(db_path, &snapshot, record_stamp, record_sha256).and_then(
            |_| {
                if visual_inputs::file_stamp(db_path).as_ref() == Some(record_stamp) {
                    Ok(())
                } else {
                    Err("frozen record changed after its capture snapshot")
                }
            },
        )
    };
    if let Err(reason) = snapshot_result {
        for receipt in &mut receipts {
            if receipt.route_found {
                receipt.failure = Some(reason.into());
            }
        }
        return receipts;
    }
    drop(snapshot_stage);
    let native_upgrade_stage = profile.span("native_upgrade");
    if let Err(reason) =
        visual_record::prepare_exact_snapshot(&server_bin, &snapshot, work, witness_event_id)
    {
        for receipt in &mut receipts {
            if receipt.route_found {
                receipt.failure = Some(reason.into());
            }
        }
        return receipts;
    }
    drop(native_upgrade_stage);
    let export_stage = profile.span("export");
    if export_exact_record(&server_bin, &snapshot, work, capture_clock).is_err() {
        for receipt in &mut receipts {
            if receipt.route_found {
                receipt.failure = Some("exact pinned record export failed".into());
            }
        }
        return receipts;
    }
    drop(export_stage);
    let server_stage = profile.span("server_start");
    let Ok((mut server, origin)) = start_exact_server(&server_bin, &snapshot, &dist, work) else {
        for receipt in &mut receipts {
            if receipt.route_found {
                receipt.failure = Some("exact server could not start".into());
            }
        }
        return receipts;
    };
    drop(server_stage);
    let capture_stage = profile.span("capture_total");
    let capture_result = capture_routes(BrowserCapture {
        origin: &origin,
        renderer,
        params,
        receipts: &receipts,
        work,
        capture_clock,
        fixture_sha256,
        profile,
    });
    drop(capture_stage);
    let _ = server.kill();
    let _ = server.wait();
    match capture_result {
        Ok(shots) => {
            let by_name: BTreeMap<_, _> = shots
                .into_iter()
                .map(|shot| (shot.screen.clone(), shot))
                .collect();
            for receipt in &mut receipts {
                if !receipt.route_found || receipt.failure.is_some() {
                    continue;
                }
                if let Some(shot) = by_name.get(&receipt.screen) {
                    receipt.screenshot_path = shot.screenshot_path.clone();
                    receipt.failure = shot.error.clone();
                    receipt.capture_clock = shot.capture_clock.clone();
                    receipt.renderer_version = shot.renderer_version.clone();
                    receipt.fixture_sha256 = shot.fixture_sha256.clone();
                    receipt.frame_width = shot.frame_width;
                    receipt.frame_height = shot.frame_height;
                    receipt.content_width = shot.content_width;
                    receipt.content_height = shot.content_height;
                    receipt.overflow_x = shot.overflow_x;
                    receipt.overflow_y = shot.overflow_y;
                    receipt.scroll_regions = shot.scroll_regions.clone();
                    receipt.clipped_regions = shot.clipped_regions.clone();
                    receipt.housekeeping_responses = shot.housekeeping_responses.clone();
                } else {
                    receipt.failure = Some("screen capture is missing".into());
                }
            }
        }
        Err(reason) => {
            for receipt in &mut receipts {
                if receipt.route_found && receipt.failure.is_none() {
                    receipt.failure = Some(reason.into());
                }
            }
        }
    }
    if receipts.iter().all(|receipt| receipt.failure.is_none()) {
        *preserved_build = visual_history::preserve_build(cache, &dist, &server_bin).ok();
    }
    receipts
}

/// Keep build diagnostics outside the scratch tree, which is removed as soon
/// as the check returns. The log is private and bounded, and is never copied
/// into a verdict or API response.
fn write_private_build_diagnostic(output: &std::process::Output) {
    let root = std::env::temp_dir().join("thruline-c1-build-diagnostics");
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    if builder.create(&root).is_err() {
        return;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if std::fs::metadata(&root).map_or(true, |meta| meta.permissions().mode() & 0o077 != 0) {
            return;
        }
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |time| time.as_nanos());
    let path = root.join(format!("cargo-{}-{stamp}.log", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    if let Ok(mut file) = options.open(path) {
        let _ = writeln!(file, "status: {}", output.status);
        let out_start = output.stdout.len().saturating_sub(4096);
        let err_start = output.stderr.len().saturating_sub(65_536);
        let _ = file.write_all(&output.stdout[out_start..]);
        let _ = file.write_all(&output.stderr[err_start..]);
    }
}

fn export_exact_record(
    server_bin: &Path,
    snapshot: &Path,
    work: &Path,
    capture_clock: &str,
) -> Result<(), ()> {
    let export_dir = work.join("export");
    std::fs::create_dir_all(&export_dir).map_err(|_| ())?;
    let mut child = Command::new(server_bin)
        .arg("--db")
        .arg(snapshot)
        .args(["export", "--all", "--out"])
        .arg(&export_dir)
        .arg("--generated-at")
        .arg(capture_clock)
        .env("THRULINE_HOME", work.join("home"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| ())?;
    let deadline = Instant::now() + EXACT_SERVER_START_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(_)) | Err(_) => return Err(()),
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(250)),
        }
    }
    export_dir
        .join("index.json")
        .is_file()
        .then_some(())
        .ok_or(())
}

fn start_exact_server(
    server_bin: &Path,
    snapshot: &Path,
    dist: &Path,
    work: &Path,
) -> Result<(Child, String), ()> {
    let mut child = Command::new(server_bin)
        .arg("--db")
        .arg(snapshot)
        .args([
            "ui",
            "--no-open",
            "--port",
            "0",
            "--address",
            "127.0.0.1",
            "--static-dir",
        ])
        .arg(dist)
        .arg("--dir")
        .arg(work.join("export"))
        .env("THRULINE_HOME", work.join("home"))
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| ())?;
    let stdout = child.stdout.take().ok_or(())?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Some(url) = line.trim().strip_prefix("ThruLine is at ") {
                let _ = tx.send(url.to_owned());
            }
        }
    });
    match rx.recv_timeout(EXACT_SERVER_START_TIMEOUT) {
        Ok(url) => {
            let origin = url.split_once('/').map(|_| url.clone()).unwrap_or(url);
            let origin = origin
                .split_once("127.0.0.1:")
                .and_then(|(scheme, rest)| {
                    rest.split(|ch: char| !ch.is_ascii_digit())
                        .next()
                        .filter(|port| !port.is_empty())
                        .map(|port| format!("{scheme}127.0.0.1:{port}"))
                })
                .ok_or(())?;
            Ok((child, origin))
        }
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(())
        }
    }
}

struct BrowserCapture<'a> {
    origin: &'a str,
    renderer: &'a Path,
    params: &'a VisualMatchParams,
    receipts: &'a [CaptureReceipt],
    work: &'a Path,
    capture_clock: &'a str,
    fixture_sha256: &'a str,
    profile: &'a VisualProfile,
}

fn capture_routes(context: BrowserCapture<'_>) -> Result<Vec<CapturedShot>, &'static str> {
    let BrowserCapture {
        origin,
        renderer,
        params,
        receipts,
        work,
        capture_clock,
        fixture_sha256,
        profile,
    } = context;
    let jobs: Vec<_> = params
        .screens
        .iter()
        .zip(receipts.iter())
        .filter(|(_, receipt)| receipt.route_found && receipt.failure.is_none())
        .map(|(screen, receipt)| CaptureJob {
            screen: &screen.screen,
            route: receipt.actual_route.as_deref().unwrap_or(&screen.route),
            scenario: receipt
                .actual_scenario
                .as_deref()
                .unwrap_or(&screen.scenario),
            width: receipt.frame_width.unwrap_or(0),
            height: receipt.frame_height.unwrap_or(0),
            full_page: false,
            output: work
                .join("shots")
                .join(format!("{}.png", digest(screen.screen.as_bytes()))),
        })
        .collect();
    let input = work.join("capture.json");
    let script = work.join("visual_capture.mjs");
    std::fs::write(&script, CAPTURE_SCRIPT).map_err(|_| "capture tool is unavailable")?;
    let body = json!({"origin": origin, "renderer": renderer, "renderer_version": APPROVED_RENDERER_PRODUCT, "capture_clock": capture_clock, "fixture_sha256": fixture_sha256, "profile_path": profile.path(), "jobs": jobs});
    std::fs::write(&input, body.to_string()).map_err(|_| "capture job is unavailable")?;
    let output = visual_process::capture_process(
        Command::new("node").arg(&script).arg(&input),
        jobs.len(),
        visual_progress::current_observer(),
        EXACT_SERVER_START_TIMEOUT,
    )?;
    if !output.status.success() {
        return Err("capture tool failed");
    }
    serde_json::from_slice(&output.stdout).map_err(|_| "capture output is invalid")
}

/// Read the exact source router and decide whether a local address names a
/// route. A SPA fallback alone is not route evidence.
pub fn route_in_version(repo: &Path, version_id: &str, route: &str) -> Option<bool> {
    if !(route.starts_with("#/") || route.starts_with('/'))
        || route.starts_with("//")
        || route.contains("://")
        || route.contains("..")
        || route.contains('\\')
    {
        return None;
    }
    let commit_oid = git_oid(repo, &format!("{version_id}^{{commit}}"))?;
    let main = git_text(repo, &format!("{commit_oid}:ui/src/main.jsx"))?;
    let hq = main.contains("./hq/app.jsx")
        && git_text(repo, &format!("{commit_oid}:ui/src/hq/routes.js")).is_some();
    let (path, hash) = if route.starts_with('#') {
        ("/", route.to_owned())
    } else if let Some((path, hash)) = route.split_once('#') {
        (path, format!("#{hash}"))
    } else {
        (route, String::new())
    };
    if ["/sessions", "/verification", "/acceptance"].contains(&path) {
        return Some(true);
    }
    if path != "/" && !path.starts_with("/?") {
        return Some(false);
    }
    let route = hash.split('?').next().unwrap_or(&hash);
    if let Some(name) = route.strip_prefix("#/design/") {
        let boards = git_text(repo, &format!("{commit_oid}:ui/src/hq/artboards.jsx"));
        return Some(
            hq && main.contains("VITE_HQ_DESIGN")
                && boards.is_some_and(|source| {
                    !name.is_empty() && source.contains(&format!("  '{name}',"))
                }),
        );
    }
    if ["", "#", "#/", "#/inbox", "#/start"].contains(&route) {
        return Some(true);
    }
    let segments: Vec<_> = route.trim_start_matches("#/").split('/').collect();
    if hq {
        if segments.len() == 1 {
            return Some(["record", "settings", "team"].contains(&segments[0]));
        }
        if segments.len() == 3 && segments[0] == "changes" && route_id(segments[1]) {
            return Some(
                ["statements", "changed", "done", "check", "approve", "story"]
                    .contains(&segments[2]),
            );
        }
        return Some(segments.len() == 2 && segments[0] == "changes" && route_id(segments[1]));
    }
    if route == "#/licence" {
        return Some(true);
    }
    if segments.len() == 1 && segments[0].bytes().all(|byte| byte.is_ascii_lowercase()) {
        let rail = git_text(repo, &format!("{commit_oid}:ui/src/workspace/logic.js"));
        let shell = git_text(repo, &format!("{commit_oid}:ui/src/workspace/shell.jsx"));
        if shell.is_some_and(|source| source.contains("routeFromHash"))
            && rail.is_some_and(|source| source.contains(&format!("['{}',", segments[0])))
        {
            return Some(true);
        }
    }
    if segments.len() == 2 && segments[0] == "changes" && route_id(segments[1]) {
        return Some(true);
    }
    if segments.len() == 3 && segments[0] == "changes" && route_id(segments[1]) {
        return Some(
            [
                "define", "prepare", "build", "verify", "review", "approve", "packet",
            ]
            .contains(&segments[2]),
        );
    }
    Some(false)
}

fn route_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn git_text(repo: &Path, object: &str) -> Option<String> {
    let output = crate::git_isolation::isolate_config(&mut Command::new("git"))
        .args(["show", "--no-ext-diff", "--format=", object])
        .current_dir(repo)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8(output.stdout).ok())
        .flatten()
}

/// Deterministic C1 verdict over typed captures. Used by the production
/// adapter and by focused tests without launching a browser.
pub fn check_visual_with(
    repo: &Path,
    version_id: &str,
    params: &VisualMatchParams,
    runtime: &VisualRuntime,
) -> VisualReport {
    let mut report = check_visual_with_raw(repo, version_id, params, runtime);
    // Input labels bind the capture using the original bytes. Mask only the
    // returned presentation, including every early Unverifiable result.
    for screen in &mut report.screens {
        screen.screen = Masked::mask(&screen.screen).into_text();
        screen.route = Masked::mask(&screen.route).into_text();
        if let Some(route) = &mut screen.actual_route {
            *route = Masked::mask(route).into_text();
        }
    }
    report.outcome.visual = report.screens.clone();
    report
}

fn check_visual_with_raw(
    repo: &Path,
    version_id: &str,
    params: &VisualMatchParams,
    runtime: &VisualRuntime,
) -> VisualReport {
    let Some(commit_oid) = git_oid(repo, &format!("{version_id}^{{commit}}")) else {
        return unavailable_report(params, "version cannot be resolved to a commit", None);
    };
    let Some(tree_hash) = git_oid(repo, &format!("{commit_oid}^{{tree}}")) else {
        return unavailable_report(params, "version tree cannot be resolved", None);
    };
    let Some(pin) = runtime
        .renderer_sha256
        .as_deref()
        .filter(|pin| is_sha256(pin))
    else {
        return unavailable_report(params, "renderer not found", Some(tree_hash));
    };
    if params.screens.is_empty() || !params.tolerance_pct.is_finite() {
        return unavailable_report(
            params,
            "visual check parameters are invalid",
            Some(tree_hash),
        );
    }
    let mut by_screen = BTreeMap::<&str, &CaptureReceipt>::new();
    for capture in &runtime.captures {
        if by_screen.insert(&capture.screen, capture).is_some() {
            return unavailable_report(params, "duplicate screen capture", Some(tree_hash));
        }
    }
    let mut screens = Vec::with_capacity(params.screens.len());
    let context = ScreenCheckContext {
        reference_root: &runtime.reference_root,
        diff_root: runtime.diff_root.as_deref(),
        commit_oid: &commit_oid,
        tree_hash: &tree_hash,
        renderer_pin: pin,
        renderer_version: runtime.renderer_version.as_deref(),
        tolerance_pct: params.tolerance_pct,
        capture_clock: runtime.capture_clock.as_deref(),
        fixture_sha256: runtime.fixture_sha256.as_deref(),
    };
    for screen in &params.screens {
        let capture = by_screen.get(screen.screen.as_str()).copied();
        screens.push(check_screen(screen, capture, &context));
    }
    report_from_screens(screens, tree_hash)
}

fn report_from_screens(screens: Vec<VisualScreenResult>, tree_hash: String) -> VisualReport {
    let state = if screens
        .iter()
        .any(|screen| screen.state == VerdictState::Broken)
    {
        VerdictState::Broken
    } else if screens
        .iter()
        .any(|screen| screen.state == VerdictState::Unverifiable)
    {
        VerdictState::Unverifiable
    } else {
        VerdictState::Holds
    };
    let findings: Vec<Finding> = screens
        .iter()
        .filter(|screen| screen.state == VerdictState::Broken)
        .map(|screen| Finding {
            surface: Surface::UiCopy,
            path: Masked::mask(screen.actual_route.as_deref().unwrap_or(&screen.route)).into_text(),
            line: 0,
            span: Masked::mask(&format!(
                "{} · {} · reference {} · built from {}{}",
                screen.screen,
                screen
                    .difference_pct
                    .map(|pct| {
                        format!(
                            "{pct:.2}% differ{}",
                            screen
                                .reason
                                .as_deref()
                                .map(|reason| format!(" · {reason}"))
                                .unwrap_or_default()
                        )
                    })
                    .unwrap_or_else(|| screen.reason.clone().unwrap_or_default()),
                screen.reference_sha256,
                screen.commit_oid.as_deref().unwrap_or("unknown"),
                screen
                    .diff_image_path
                    .as_ref()
                    .map(|path| format!(" · diff {path}"))
                    .unwrap_or_default()
            ))
            .into_text(),
        })
        .collect();
    let checked_places = screens.len() as u64;
    let unverifiable_count = screens
        .iter()
        .filter(|screen| screen.state == VerdictState::Unverifiable)
        .count();
    let reason = match state {
        VerdictState::Holds => None,
        VerdictState::Broken => Some(format!(
            "{} of {} screens differ or have no route; {} unverifiable",
            findings.len(),
            screens.len(),
            unverifiable_count
        )),
        VerdictState::Unverifiable => Some(format!(
            "{} of {} screen captures could not be verified",
            unverifiable_count,
            screens.len()
        )),
        VerdictState::NotApplicable => None,
    };
    VisualReport {
        outcome: CheckOutcome {
            state,
            tree_hash: Some(tree_hash),
            reason,
            checked_places,
            findings,
            surfaces: vec![SurfaceResult {
                surface: Surface::UiCopy,
                checked_places,
                finding_count: screens
                    .iter()
                    .filter(|screen| screen.state == VerdictState::Broken)
                    .count() as u64,
                state,
            }],
            not_looked_at: Vec::new(),
            visual: screens.clone(),
        },
        screens,
    }
}

#[derive(Clone, Copy)]
struct ScreenCheckContext<'a> {
    reference_root: &'a Path,
    diff_root: Option<&'a Path>,
    commit_oid: &'a str,
    tree_hash: &'a str,
    renderer_pin: &'a str,
    renderer_version: Option<&'a str>,
    tolerance_pct: f64,
    capture_clock: Option<&'a str>,
    fixture_sha256: Option<&'a str>,
}

fn check_screen(
    screen: &VisualScreen,
    capture: Option<&CaptureReceipt>,
    context: &ScreenCheckContext<'_>,
) -> VisualScreenResult {
    let ScreenCheckContext {
        reference_root,
        diff_root,
        commit_oid,
        tree_hash,
        renderer_pin,
        renderer_version,
        tolerance_pct,
        capture_clock,
        fixture_sha256,
    } = *context;
    let mut result = VisualScreenResult {
        screen: screen.screen.clone(),
        route: screen.route.clone(),
        actual_route: None,
        actual_scenario: None,
        state: VerdictState::Unverifiable,
        difference_pct: None,
        different_pixels: None,
        checked_pixels: None,
        reference_width: None,
        reference_height: None,
        capture_width: None,
        capture_height: None,
        compared_width: None,
        compared_height: None,
        diff_image_path: None,
        diff_sha256: None,
        reference_sha256: screen.reference_sha256.clone(),
        version_id: None,
        commit_oid: None,
        tree_hash: None,
        renderer_sha256: None,
        renderer_version: None,
        capture_clock: None,
        fixture_sha256: None,
        frame_width: None,
        frame_height: None,
        content_width: None,
        content_height: None,
        overflow_x: None,
        overflow_y: None,
        scroll_regions: Vec::new(),
        clipped_regions: Vec::new(),
        housekeeping_responses: Vec::new(),
        reason: None,
    };
    let Some(capture) = capture else {
        result.reason = Some("screen capture is missing".into());
        return result;
    };
    if capture.commit_oid != commit_oid || capture.tree_hash != tree_hash {
        result.reason = Some("screen capture is from a different version".into());
        return result;
    }
    result.commit_oid = Some(commit_oid.into());
    result.version_id = Some(commit_oid.into());
    result.tree_hash = Some(tree_hash.into());
    let relative = Path::new(&screen.reference_path);
    if !safe_relative_path(relative) || !is_sha256(&screen.reference_sha256) {
        result.reason = Some("reference pin is invalid".into());
        return result;
    }
    let reference = reference_root.join(relative);
    let reference_stays_inside_root = reference_root
        .canonicalize()
        .ok()
        .zip(reference.canonicalize().ok())
        .is_some_and(|(root, file)| file.starts_with(root));
    if !reference_stays_inside_root {
        result.reason = Some("reference path leaves its root or is missing".into());
        return result;
    }
    let reference_stamp = visual_inputs::file_stamp(&reference);
    let Some(reference_hash) = file_sha256(&reference) else {
        result.reason = Some("reference image is missing".into());
        return result;
    };
    if !reference_hash.eq_ignore_ascii_case(&screen.reference_sha256) {
        result.reason = Some("reference image hash does not match".into());
        return result;
    }
    if !screen.mask_rectangles.is_empty() {
        result.reason = Some("C1 reference frames do not permit masks".into());
        return result;
    }
    let Some((frame_width, frame_height)) = png_dimensions(&reference) else {
        result.reason = Some("reference PNG frame is invalid".into());
        return result;
    };
    // An exact-tree absence is a source fact. It has no browser capture and
    // must retain null runtime/clock/geometry fields rather than invent them.
    if !capture.route_found {
        if capture.route != screen.route || capture.scenario != screen.scenario {
            result.reason = Some("screen capture route or scenario does not match".into());
            return result;
        }
        if let Some(failure) = &capture.failure {
            result.reason = Some(Masked::mask(failure).into_text());
            return result;
        }
        result.state = VerdictState::Broken;
        result.reason = Some("route not found".into());
        return result;
    }
    // Preparation can fail before a browser exists. Retain that named need
    // without presenting placeholder fields as observations of a runtime.
    if let Some(failure) = &capture.failure {
        result.reason = Some(Masked::mask(failure).into_text());
        return result;
    }
    if capture.renderer_sha256 != renderer_pin {
        result.reason = Some("renderer pin does not match capture".into());
        return result;
    }
    result.renderer_sha256 = Some(renderer_pin.into());
    result.renderer_version = capture.renderer_version.clone();
    if renderer_version.is_some() && capture.renderer_version.as_deref() != renderer_version {
        result.reason = Some("capture renderer version does not match the approved runtime".into());
        return result;
    }
    result.capture_clock = capture.capture_clock.clone();
    result.fixture_sha256 = capture.fixture_sha256.clone();
    result.frame_width = capture.frame_width;
    result.frame_height = capture.frame_height;
    result.content_width = capture.content_width;
    result.content_height = capture.content_height;
    result.overflow_x = capture.overflow_x;
    result.overflow_y = capture.overflow_y;
    result.scroll_regions = capture.scroll_regions.clone();
    result.clipped_regions = capture.clipped_regions.clone();
    result.housekeeping_responses = capture.housekeeping_responses.clone();
    if capture.housekeeping_responses.len() > 64
        || capture
            .housekeeping_responses
            .iter()
            .any(|response| response.path != "/favicon.ico" || response.status != 204)
    {
        result.reason =
            Some("capture housekeeping response is outside the declared static contract".into());
        return result;
    }
    if capture_clock.is_none_or(|clock| !valid_utc_second(clock))
        || capture.capture_clock.as_deref() != capture_clock
    {
        result.reason = Some("capture clock is missing or differs from the frozen record".into());
        return result;
    }
    if fixture_sha256.is_none_or(|hash| !is_sha256(hash))
        || capture.fixture_sha256.as_deref() != fixture_sha256
    {
        result.reason = Some("capture fixture does not match the frozen record manifest".into());
        return result;
    }
    if capture.route != screen.route || capture.scenario != screen.scenario {
        result.reason = Some("screen capture route or scenario does not match".into());
        return result;
    }
    let actual_route = capture.actual_route.as_deref().unwrap_or(&capture.route);
    let actual_scenario = capture
        .actual_scenario
        .as_deref()
        .unwrap_or(&capture.scenario);
    let primary = actual_route == screen.route && actual_scenario == screen.scenario;
    let legacy = screen.legacy_route.as_deref() == Some(actual_route)
        && screen.legacy_scenario.as_deref() == Some(actual_scenario);
    if !primary && !legacy {
        result.reason = Some("screen capture used an unaccepted route or scenario".into());
        return result;
    }
    result.actual_route = Some(actual_route.into());
    result.actual_scenario = Some(actual_scenario.into());
    let Some(screenshot) = capture.screenshot_path.as_ref() else {
        result.reason = Some("screen capture is missing".into());
        return result;
    };
    let (Some(content_width), Some(content_height), Some(overflow_x), Some(overflow_y)) = (
        capture.content_width,
        capture.content_height,
        capture.overflow_x,
        capture.overflow_y,
    ) else {
        result.reason = Some("capture content geometry is missing".into());
        return result;
    };
    let regions_valid = |regions: &[ScrollRegion]| {
        regions.len() <= 8192
            && regions.iter().all(|region| {
                [region.x, region.y, region.width, region.height]
                    .iter()
                    .all(|number| number.is_finite())
                    && region.width >= 0.0
                    && region.height >= 0.0
            })
    };
    if !regions_valid(&capture.scroll_regions) || !regions_valid(&capture.clipped_regions) {
        result.reason = Some("capture scroll or clipping geometry is invalid".into());
        return result;
    }
    if capture.frame_width != Some(frame_width)
        || capture.frame_height != Some(frame_height)
        || content_width == 0
        || content_height == 0
    {
        result.reason = Some("capture frame does not match the accepted reference PNG".into());
        return result;
    }
    let profile = VisualProfile::for_stage();
    let _diff_stage = profile.span("diff");
    let capture_stamp = visual_inputs::file_stamp(screenshot);
    if capture_stamp.is_none() {
        result.reason = Some("captured PNG is unavailable".into());
        return result;
    }
    let comparison = compare_images(&reference, screenshot, screen, diff_root, commit_oid);
    if reference_stamp.is_none() || visual_inputs::file_stamp(&reference) != reference_stamp {
        result.reason = Some("reference PNG changed during comparison".into());
        return result;
    }
    if visual_inputs::file_stamp(screenshot) != capture_stamp {
        result.reason = Some("captured PNG changed during comparison".into());
        return result;
    }
    match comparison {
        Ok(comparison) => {
            result.difference_pct = Some(comparison.difference_pct);
            result.different_pixels = Some(comparison.different_pixels);
            result.checked_pixels = Some(comparison.checked_pixels);
            result.reference_width = Some(comparison.reference_width);
            result.reference_height = Some(comparison.reference_height);
            result.capture_width = Some(comparison.capture_width);
            result.capture_height = Some(comparison.capture_height);
            result.compared_width = Some(comparison.compared_width);
            result.compared_height = Some(comparison.compared_height);
            result.diff_image_path = comparison.diff_image_path;
            result.diff_sha256 = comparison.diff_sha256;
            let accepted_dimensions = comparison.capture_width == frame_width
                && comparison.capture_height == frame_height;
            if comparison.dimensions_differ || !accepted_dimensions {
                result.state = VerdictState::Broken;
                result.reason = Some("screen dimensions differ".into());
                return result;
            }
            if overflow_x
                || overflow_y
                || content_width > frame_width
                || content_height > frame_height
            {
                result.state = VerdictState::Broken;
                result.reason =
                    Some("screen content overflows the accepted reference frame".into());
                return result;
            }
            result.state = if comparison.difference_pct <= tolerance_pct {
                VerdictState::Holds
            } else {
                VerdictState::Broken
            };
        }
        Err(reason) => {
            result.state = VerdictState::Unverifiable;
            result.reason = Some(reason.into());
        }
    }
    result
}

#[derive(Debug, Deserialize)]
struct PixelComparison {
    different_pixels: u64,
    checked_pixels: u64,
    difference_pct: f64,
    reference_width: u32,
    reference_height: u32,
    capture_width: u32,
    capture_height: u32,
    compared_width: u32,
    compared_height: u32,
    dimensions_differ: bool,
    comparison_sha256: String,
    #[serde(skip)]
    diff_image_path: Option<String>,
    #[serde(skip)]
    diff_sha256: Option<String>,
}

fn compare_images(
    reference: &Path,
    screenshot: &Path,
    screen: &VisualScreen,
    diff_root: Option<&Path>,
    commit_oid: &str,
) -> Result<PixelComparison, &'static str> {
    let masks =
        serde_json::to_string(&screen.mask_rectangles).map_err(|_| "mask list is invalid")?;
    let staging = diff_root
        .map(|_| tempfile::tempdir().map_err(|_| "diff scratch space is unavailable"))
        .transpose()?;
    let staged_path = staging.as_ref().map(|dir| dir.path().join("diff.png"));
    let mut comparator = Command::new(
        std::env::var_os("THRULINE_VISUAL_PYTHON").unwrap_or_else(|| "python3".into()),
    );
    if let Some(base) = build_caches().and_then(|cache| python_userbase(&cache.account_home)) {
        comparator.env("PYTHONUSERBASE", base);
    }
    let output = comparator
        .arg("-c")
        .arg(COMPARE_SCRIPT)
        .arg(reference)
        .arg(screenshot)
        .arg(staged_path.as_deref().unwrap_or_else(|| Path::new("")))
        .arg(&masks)
        .output()
        .map_err(|_| "pixel comparator is unavailable")?;
    if !output.status.success() {
        return Err(
            if String::from_utf8_lossy(&output.stderr).contains("ModuleNotFoundError") {
                "pixel comparator dependencies are unavailable"
            } else {
                "pixel comparator failed"
            },
        );
    }
    let value: Value =
        serde_json::from_slice(&output.stdout).map_err(|_| "pixel comparator output is invalid")?;
    if let Some(error) = value["error"].as_str() {
        return Err(match error {
            "invalid_mask" => "accepted mask is outside the screen",
            "all_pixels_masked" => "accepted mask covers the whole screen",
            _ => "pixel comparator failed",
        });
    }
    let mut comparison: PixelComparison =
        serde_json::from_value(value).map_err(|_| "pixel comparator output is invalid")?;
    if !comparison.difference_pct.is_finite()
        || comparison.difference_pct < 0.0
        || comparison.difference_pct > 100.0
    {
        return Err("pixel comparator output is invalid");
    }
    if !is_sha256(&comparison.comparison_sha256) {
        return Err("pixel comparator output is invalid");
    }
    if let (Some(root), Some(staged)) = (diff_root, staged_path.as_deref()) {
        let safe_name: String = screen
            .screen
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || ch == '-' {
                    ch
                } else {
                    '_'
                }
            })
            .collect();
        let key = digest(
            format!(
                "{}\n{}\n{}\n{}\n{}\n{}",
                screen.route,
                screen.scenario,
                screen.reference_sha256,
                comparison.comparison_sha256,
                masks,
                digest(COMPARE_SCRIPT.as_bytes())
            )
            .as_bytes(),
        );
        let path = root
            .join(commit_oid)
            .join(safe_name)
            .join(format!("{}.diff.png", &key[..24]));
        std::fs::create_dir_all(path.parent().ok_or("diff folder is invalid")?)
            .map_err(|_| "diff folder is unavailable")?;
        if path.is_file() {
            if file_sha256(&path) != file_sha256(staged) {
                return Err("diff artifact hash changed");
            }
        } else {
            std::fs::rename(staged, &path)
                .or_else(|_| std::fs::copy(staged, &path).map(|_| ()))
                .map_err(|_| "diff artifact could not be saved")?;
        }
        comparison.diff_sha256 = file_sha256(&path);
        comparison.diff_image_path = Some(Masked::mask(&path.to_string_lossy()).into_text());
    }
    Ok(comparison)
}

fn unavailable_report(
    params: &VisualMatchParams,
    reason: &str,
    tree_hash: Option<String>,
) -> VisualReport {
    let screens: Vec<VisualScreenResult> = params
        .screens
        .iter()
        .map(|screen| VisualScreenResult {
            screen: screen.screen.clone(),
            route: screen.route.clone(),
            actual_route: None,
            actual_scenario: None,
            state: VerdictState::Unverifiable,
            difference_pct: None,
            different_pixels: None,
            checked_pixels: None,
            reference_width: None,
            reference_height: None,
            capture_width: None,
            capture_height: None,
            compared_width: None,
            compared_height: None,
            diff_image_path: None,
            diff_sha256: None,
            reference_sha256: screen.reference_sha256.clone(),
            version_id: None,
            commit_oid: None,
            tree_hash: tree_hash.clone(),
            renderer_sha256: None,
            renderer_version: None,
            capture_clock: None,
            fixture_sha256: None,
            frame_width: None,
            frame_height: None,
            content_width: None,
            content_height: None,
            overflow_x: None,
            overflow_y: None,
            scroll_regions: Vec::new(),
            clipped_regions: Vec::new(),
            housekeeping_responses: Vec::new(),
            reason: Some(reason.into()),
        })
        .collect();
    let mut outcome = CheckOutcome::unavailable(reason, tree_hash);
    outcome.visual = screens.clone();
    VisualReport { outcome, screens }
}

fn safe_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn file_sha256(path: &Path) -> Option<String> {
    let mut reader = BufReader::new(File::open(path).ok()?);
    let mut hasher = Sha256::new();
    let mut chunk = [0u8; 64 * 1024];
    loop {
        let size = reader.read(&mut chunk).ok()?;
        if size == 0 {
            break;
        }
        hasher.update(&chunk[..size]);
    }
    Some(digest_bytes(hasher.finalize().as_slice()))
}

fn digest(input: &[u8]) -> String {
    digest_bytes(Sha256::digest(input).as_slice())
}

fn digest_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn git_oid(repo: &Path, revision: &str) -> Option<String> {
    let output = crate::git_isolation::isolate_config(&mut Command::new("git"))
        .args(["rev-parse", "--verify", "--end-of-options", revision])
        .current_dir(repo)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let oid = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    ((oid.len() == 40 || oid.len() == 64) && oid.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then_some(oid)
}

fn png_dimensions(path: &Path) -> Option<(u32, u32)> {
    let mut header = [0u8; 24];
    File::open(path).ok()?.read_exact(&mut header).ok()?;
    if header[..8] != [137, 80, 78, 71, 13, 10, 26, 10] || &header[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes(header[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(header[20..24].try_into().ok()?);
    (width >= 320 && height >= 240 && width <= 4096 && height <= 4096).then_some((width, height))
}

fn reference_frame(repo: &Path, screen: &VisualScreen) -> Option<(u32, u32)> {
    let root = std::env::var_os("THRULINE_VISUAL_REFERENCE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo.to_path_buf());
    let relative = Path::new(&screen.reference_path);
    if !safe_relative_path(relative) || !screen.mask_rectangles.is_empty() {
        return None;
    }
    let reference = root.join(relative);
    if !reference
        .canonicalize()
        .ok()?
        .starts_with(root.canonicalize().ok()?)
        || !file_sha256(&reference)?.eq_ignore_ascii_case(&screen.reference_sha256)
    {
        return None;
    }
    png_dimensions(&reference)
}

fn enough_space_for_build(path: &Path) -> bool {
    let Ok(output) = Command::new("df").arg("-Pk").arg(path).output() else {
        return false;
    };
    output.status.success()
        && String::from_utf8_lossy(&output.stdout)
            .lines()
            .nth(1)
            .and_then(|line| line.split_whitespace().nth(3))
            .and_then(|available| available.parse::<u64>().ok())
            .is_some_and(|available| available >= 25 * 1024 * 1024)
}
