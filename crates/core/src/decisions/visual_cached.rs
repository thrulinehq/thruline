//! Per-screen C1 cache over exact built assets, frozen input and approved tool.
use super::visual_cache::{CaptureKey, ResultKey, VisualCache};
use super::*;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
#[derive(Clone, Copy)]
pub(super) struct DecisionContext<'a> {
    pub id: &'a str,
    pub revision: i64,
}
pub(super) fn all_primary_closed(repo: &Path, commit: &str, params: &VisualMatchParams) -> bool {
    if !params
        .screens
        .iter()
        .all(|s| s.scenario == "design" && is_design_route(&s.route))
    {
        return false;
    }
    if !visual_build::supports_closed_design(repo, commit) {
        return false;
    }
    let Some(boards) = git_text(repo, &format!("{commit}:ui/src/hq/artboards.jsx")) else {
        return false;
    };
    params.screens.iter().all(|s| {
        s.route
            .trim_start_matches('/')
            .strip_prefix("#/design/")
            .is_some_and(|name| !name.is_empty() && boards.contains(&format!("  '{name}',")))
    })
}
pub(super) fn check_closed(
    context: CaptureContext<'_>,
    cache: &VisualCache,
    decision: DecisionContext<'_>,
    reference_root: &Path,
    diff_root: &Path,
) -> CheckOutcome {
    let params = context.params;
    let built = match visual_build::exact_design_build(
        context.repo,
        context.commit_oid,
        context.tree_hash,
        cache,
        context.profile,
    ) {
        Ok(b) => b,
        Err(r) => return unavailable_report(params, r, Some(context.tree_hash.into())).outcome,
    };
    let mut captures = Vec::with_capacity(params.screens.len());
    let mut keys = Vec::new();
    let mut results = Vec::new();
    let mut completed = 0;
    for screen in &params.screens {
        let Some((width, height)) = reference_frame(context.repo, screen) else {
            return unavailable_report(
                params,
                "accepted reference PNG frame is unavailable or does not match its pin",
                Some(context.tree_hash.into()),
            )
            .outcome;
        };
        let key = CaptureKey {
            implementation: VISUAL_IMPL_VERSION.into(),
            ui_sha256: built.ui_sha256.clone(),
            server_sha256: built.server_sha256.clone(),
            server_mode: "closed-design-artifacts-v1".into(),
            capture_tool_sha256: digest(CAPTURE_SCRIPT.as_bytes()),
            fixture_sha256: context.fixture_sha256.into(),
            renderer_sha256: context.renderer_sha256.into(),
            renderer_version: APPROVED_RENDERER_PRODUCT.into(),
            frame_width: width,
            frame_height: height,
            capture_clock: context.capture_clock.into(),
            screen: screen.screen.clone(),
            route: screen.route.clone(),
            scenario: screen.scenario.clone(),
        }
        .key();
        let result_key = ResultKey {
            capture_key: key.clone(),
            decision_id: decision.id.into(),
            revision: decision.revision,
            reference_sha256: screen.reference_sha256.clone(),
            tolerance_bits: params.tolerance_pct.to_bits(),
            compare_tool_sha256: digest(COMPARE_SCRIPT.as_bytes()),
        };
        let mut result = cache
            .load::<VisualScreenResult>("result", &result_key.key())
            .or_else(|| cache.load("comparison", &result_key.comparison_key()));
        if let Some(r) = &mut result {
            if r.state == VerdictState::Unverifiable
                || r.screen != screen.screen
                || r.reference_sha256 != screen.reference_sha256
                || r.renderer_sha256.as_deref() != Some(context.renderer_sha256)
                || r.renderer_version.as_deref() != Some(APPROVED_RENDERER_PRODUCT)
                || r.capture_clock.as_deref() != Some(context.capture_clock)
                || r.fixture_sha256.as_deref() != Some(context.fixture_sha256)
                || r.frame_width != Some(width)
                || r.frame_height != Some(height)
            {
                result = None
            } else {
                rebind(r, context.commit_oid, context.tree_hash);
                completed += 1;
                visual_progress::emit(
                    completed,
                    params.screens.len(),
                    format!(
                        "{completed} of {} screens · cached comparison",
                        params.screens.len()
                    ),
                );
            }
        }
        let mut capture = cache
            .load::<CaptureReceipt>("capture", &key)
            .filter(|c| c.failure.is_none() && c.screenshot_path.is_some());
        // A result must retain its verified pixel artifact when promoted to a
        // new revision. A missing capture receipt requires fresh capture;
        // saving a result with only reference/diff files would lose that proof.
        if result.is_some() && capture.is_none() {
            result = None;
            completed -= 1;
        }
        if let Some(c) = &mut capture {
            c.commit_oid = context.commit_oid.into();
            c.tree_hash = context.tree_hash.into();
            if result.is_none() {
                completed += 1;
                visual_progress::emit(
                    completed,
                    params.screens.len(),
                    format!(
                        "{completed} of {} screens · cached capture",
                        params.screens.len()
                    ),
                );
            }
        }
        if capture.is_none() {
            capture = Some(CaptureReceipt {
                screen: screen.screen.clone(),
                route: screen.route.clone(),
                scenario: screen.scenario.clone(),
                actual_route: Some(screen.route.clone()),
                actual_scenario: Some(screen.scenario.clone()),
                commit_oid: context.commit_oid.into(),
                tree_hash: context.tree_hash.into(),
                renderer_sha256: context.renderer_sha256.into(),
                renderer_version: Some(APPROVED_RENDERER_PRODUCT.into()),
                capture_clock: Some(context.capture_clock.into()),
                fixture_sha256: Some(context.fixture_sha256.into()),
                frame_width: Some(width),
                frame_height: Some(height),
                content_width: None,
                content_height: None,
                overflow_x: None,
                overflow_y: None,
                scroll_regions: Vec::new(),
                clipped_regions: Vec::new(),
                housekeeping_responses: Vec::new(),
                route_found: true,
                screenshot_path: None,
                failure: None,
            })
        }
        captures.push(capture.unwrap());
        results.push(result);
        keys.push((key, result_key));
    }
    let pending: Vec<_> = captures
        .iter()
        .enumerate()
        .filter(|(i, c)| results[*i].is_none() && c.screenshot_path.is_none())
        .map(|(i, _)| i)
        .collect();
    if !pending.is_empty() {
        let mut paths = built.allowed_paths.clone();
        paths.extend(built.housekeeping.iter().map(|h| h.path.clone()));
        let server = {
            let _stage = context.profile.span("server_start");
            match visual_static::StaticDesignServer::start(&built.dist, &paths) {
                Ok(s) => s,
                Err(r) => {
                    return unavailable_report(params, r, Some(context.tree_hash.into())).outcome
                }
            }
        };
        let jobs: Vec<_> = pending
            .iter()
            .map(|i| {
                let s = &params.screens[*i];
                let c = &captures[*i];
                CaptureJob {
                    screen: &s.screen,
                    route: &s.route,
                    scenario: &s.scenario,
                    width: c.frame_width.unwrap(),
                    height: c.frame_height.unwrap(),
                    full_page: false,
                    output: context.work.join("shots").join(format!("{i}.png")),
                }
            })
            .collect();
        let parent = visual_progress::current_observer();
        let total = params.screens.len();
        let base = completed;
        let observer: VisualProgressObserver = Arc::new(move |p| {
            visual_progress::emit_to(
                parent.as_ref(),
                base + p.completed,
                total,
                format!("{} of {total} screens", base + p.completed),
            );
        });
        let input = context.work.join("capture.json");
        let script = context.work.join("visual_capture.mjs");
        let input_body = json!({"origin":server.origin,"renderer":context.renderer,"renderer_version":APPROVED_RENDERER_PRODUCT,"capture_clock":context.capture_clock,"fixture_sha256":context.fixture_sha256,"profile_path":context.profile.path(),"allowed_paths":built.allowed_paths,"housekeeping_paths":built.housekeeping.iter().map(|h|h.path.clone()).collect::<Vec<_>>(),"jobs":jobs});
        if std::fs::write(&script, CAPTURE_SCRIPT).is_err()
            || std::fs::write(&input, input_body.to_string()).is_err()
        {
            return unavailable_report(
                params,
                "capture job unavailable",
                Some(context.tree_hash.into()),
            )
            .outcome;
        }
        let output = {
            let _stage = context.profile.span("capture_total");
            visual_process::capture_process(
                Command::new("node").arg(&script).arg(&input),
                pending.len(),
                Some(observer),
                EXACT_SERVER_START_TIMEOUT,
            )
        };
        let shots = output.and_then(|o| {
            if !o.status.success() {
                Err("capture tool failed")
            } else {
                serde_json::from_slice::<Vec<CapturedShot>>(&o.stdout)
                    .map_err(|_| "capture output invalid")
            }
        });
        match shots {
            Ok(shots) => {
                let by_name: BTreeMap<_, _> =
                    shots.into_iter().map(|s| (s.screen.clone(), s)).collect();
                if by_name.len() != pending.len() {
                    return unavailable_report(
                        params,
                        "capture output count or names invalid",
                        Some(context.tree_hash.into()),
                    )
                    .outcome;
                }
                for i in pending {
                    let c = &mut captures[i];
                    if let Some(s) = by_name.get(&c.screen) {
                        apply_shot(c, s);
                        if c.failure.is_none() {
                            if let Some(png) = &c.screenshot_path {
                                let dir = cache.root().join("screens");
                                let _ = std::fs::create_dir_all(&dir);
                                let target = dir.join(format!("{}.png", keys[i].0));
                                if std::fs::copy(png, &target).is_ok() {
                                    c.screenshot_path = Some(target.clone());
                                    let _ = cache.save("capture", &keys[i].0, c, &[target]);
                                }
                            }
                        }
                    } else {
                        c.failure = Some("screen capture missing".into());
                    }
                }
            }
            Err(r) => {
                for i in pending {
                    captures[i].failure = Some(r.into())
                }
            }
        }
    }
    let check_context = ScreenCheckContext {
        reference_root,
        diff_root: Some(diff_root),
        commit_oid: context.commit_oid,
        tree_hash: context.tree_hash,
        renderer_pin: context.renderer_sha256,
        renderer_version: Some(APPROVED_RENDERER_PRODUCT),
        tolerance_pct: params.tolerance_pct,
        capture_clock: Some(context.capture_clock),
        fixture_sha256: Some(context.fixture_sha256),
    };
    let mut screens = Vec::new();
    for (i, screen) in params.screens.iter().enumerate() {
        let result = results[i]
            .take()
            .unwrap_or_else(|| check_screen(screen, Some(&captures[i]), &check_context));
        let mut files = vec![reference_root.join(&screen.reference_path)];
        if let Some(png) = &captures[i].screenshot_path {
            files.push(png.clone())
        }
        if let Some(diff) = &result.diff_image_path {
            files.push(PathBuf::from(diff));
        }
        if result.state != VerdictState::Unverifiable {
            let _ = cache.save("comparison", &keys[i].1.comparison_key(), &result, &files);
            let _ = cache.save("result", &keys[i].1.key(), &result, &files);
        }
        visual_progress::emit(
            params.screens.len(),
            params.screens.len(),
            format!(
                "{} of {} screens · checking pixels ({})",
                params.screens.len(),
                params.screens.len(),
                screen.screen
            ),
        );
        screens.push(result);
    }
    report_from_screens(screens, context.tree_hash.into()).outcome
}
fn apply_shot(c: &mut CaptureReceipt, s: &CapturedShot) {
    c.screenshot_path = s.screenshot_path.clone();
    c.failure = s.error.clone();
    c.capture_clock = s.capture_clock.clone();
    c.fixture_sha256 = s.fixture_sha256.clone();
    c.renderer_version = s.renderer_version.clone();
    c.frame_width = s.frame_width;
    c.frame_height = s.frame_height;
    c.content_width = s.content_width;
    c.content_height = s.content_height;
    c.overflow_x = s.overflow_x;
    c.overflow_y = s.overflow_y;
    c.scroll_regions = s.scroll_regions.clone();
    c.clipped_regions = s.clipped_regions.clone();
    c.housekeeping_responses = s.housekeeping_responses.clone();
}
fn rebind(r: &mut VisualScreenResult, commit: &str, tree: &str) {
    r.commit_oid = Some(commit.into());
    r.version_id = Some(commit.into());
    r.tree_hash = Some(tree.into());
}
