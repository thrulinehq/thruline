//! AI-REVIEW-OUT-OF-LAUNCH-R5 — the `ai-review` build is unchanged.
//!
//! Round 5 turned the launch build's rule into an allow-list: an id is named
//! in what is served or printed only when the build ships it, and every door
//! that echoes an id refuses the ones it does not. None of that is the
//! `ai-review` build's business. It offers the PR-Agent review, so it still
//! names what an operator registers, still lets
//! `agent-repair` and `decode` reach the review, and its route still hands an
//! id it does not know to the tool that owns the answer.
//!
//! This file drives the SAME inputs as the launch build's tests
//! (`ai_review_out_of_launch.rs`) through the feature build and pins what
//! that build does with them. It is green at the commit before round 5's fix
//! and green after it. Compiled only with `--features ai-review`.
//!
//! Every fixture is synthetic.
#![cfg(all(target_os = "macos", feature = "ai-review"))]

use serde_json::{json, Value};

#[path = "support/review_fixture.rs"]
mod fixture;
use fixture::{Fixture, Server, NOT_A_SPELLING, ROUND_4, ROUND_5};

/// The launch build's words. This build says none of them.
const LAUNCH_ONLY: &[&str] = &[
    "this build runs",
    "This build does not run that check.",
    "AI review is not provisioned in this build of ThruLine",
];

fn every_id() -> Vec<&'static str> {
    ROUND_4
        .iter()
        .chain(ROUND_5)
        .chain(NOT_A_SPELLING)
        .copied()
        .collect()
}

fn says_nothing_launch_only(text: &str) {
    for words in LAUNCH_ONLY {
        assert!(
            !text.contains(words),
            "the ai-review build says a launch-only sentence ({words:?}): {text}"
        );
    }
}

#[test]
fn a_registry_selection_of_any_id_is_named_as_it_always_was() {
    let f = Fixture::new();
    for id in every_id() {
        f.select(&["ast-grep", id]);
        let (code, out, err) = f.review("preview", "full", "");
        assert_eq!(code, 0, "{id:?}: {out}{err}");
        says_nothing_launch_only(&out);
        let plan: Value = serde_json::from_str(&out).expect("json");
        assert_eq!(
            plan["project"]["recipes"],
            json!(["ast-grep", id]),
            "the selection is served whole: {out}"
        );
        assert!(
            plan["gaps"]
                .as_array()
                .expect("gaps")
                .iter()
                .any(|g| g["recipe"] == json!(id)
                    && g["reason"]
                        == json!("No native guided adapter exists for this registered selection")),
            "{id:?} is named in its gap, as before: {out}"
        );
    }
}

#[test]
fn the_route_names_a_selection_as_it_always_did() {
    let f = Fixture::new();
    let mut all = vec!["ast-grep"];
    let ids = every_id();
    all.extend(&ids);
    f.select(&all);
    let server = Server::start(&f);
    let request =
        json!({"session": "review-t", "action": "review-preview", "recipe": "full"}).to_string();
    let (status, body) = server.request("POST", "/api/integrations/action", Some(&request));
    assert_eq!(status, 200, "{body}");
    says_nothing_launch_only(&body);
    let plan: Value = serde_json::from_str(&body).expect("json");
    assert_eq!(
        plan["project"]["recipes"].as_array().map(Vec::len),
        Some(all.len()),
        "every selection is served: {body}"
    );

    // An id it does not know is not the route's to refuse: the tool that owns
    // the recipe answers it, as it did before round 5.
    let request =
        json!({"session": "review-t", "action": "review-preview", "recipe": "made-up-tool"})
            .to_string();
    let (status, body) = server.request("POST", "/api/integrations/action", Some(&request));
    assert_eq!(status, 409, "{body}");
    assert!(body.contains("Choose full or change review"), "{body}");
    says_nothing_launch_only(&body);
    for id in ["PR-Agent", ROUND_5[0], "qodo-merge"] {
        let request = json!({"session": "review-t", "action": "run", "recipe": id}).to_string();
        let (status, body) = server.request("POST", "/api/integrations/action", Some(&request));
        assert_eq!(status, 409, "{id:?}: {body}");
        says_nothing_launch_only(&body);
    }
}

#[test]
fn a_review_mode_is_refused_as_it_always_was() {
    let f = Fixture::new();
    let (code, out, err) = f.review("preview", "made-up-tool", "");
    assert_ne!(code, 0, "{out}");
    says_nothing_launch_only(&err);
    assert!(err.contains("Choose full or change review"), "{err}");
}

#[test]
fn agent_repair_and_decode_reach_the_review_as_they_always_did() {
    let f = Fixture::new();
    let root = f.root.display().to_string();
    let (code, out, err) = f.run(&[
        "integration",
        "agent-repair",
        "--root",
        &root,
        "--action",
        "list",
        "--recipe",
        "pr-agent",
    ]);
    assert_eq!(code, 0, "{out}{err}");
    let listed: Value = serde_json::from_str(&out).expect("json");
    assert_eq!(listed["repairs"], json!([]), "{out}");

    let (code, out, err) = f.run(&[
        "integration",
        "agent-repair",
        "--root",
        &root,
        "--action",
        "preview",
        "--recipe",
        "pr-agent",
    ]);
    assert_eq!(code, 0, "{out}{err}");
    says_nothing_launch_only(&format!("{out}{err}"));
    let attempt: Value = serde_json::from_str(&out).expect("json");
    assert_eq!(attempt["recipe"], json!("pr-agent"), "{out}");
    assert_eq!(attempt["state"], json!("unavailable"), "{out}");
    assert!(
        f.db.with_extension("integration-agent-repairs")
            .join("attempts")
            .is_dir(),
        "and it records its attempt"
    );

    // decode finds the pack and reads the report (here, one that is not a
    // PR-Agent capture) rather than refusing the pack.
    let report = f.root.join("..").join("report.json");
    std::fs::write(&report, "{}").expect("report");
    let (code, out, err) = f.run(&[
        "integration",
        "decode",
        "pr-agent",
        &report.display().to_string(),
    ]);
    assert_ne!(code, 0, "{out}");
    says_nothing_launch_only(&err);
    assert!(
        err.contains("pinned source"),
        "it reached the decoder: {err}"
    );
}

#[test]
fn the_catalog_and_the_workflow_still_offer_the_review() {
    let f = Fixture::new();
    let (code, out, err) = f.run(&["integration", "catalog"]);
    assert_eq!(code, 0, "{err}");
    let catalog: Value = serde_json::from_str(&out).expect("json");
    assert!(
        catalog["projects"]
            .as_array()
            .expect("projects")
            .iter()
            .any(|p| p["id"] == json!("pr-agent")),
        "the source list keeps the PR-Agent row"
    );
    let root = f.root.display().to_string();
    let exec = f.exec.display().to_string();
    let (code, out, err) = f.run(&[
        "integration",
        "workflow",
        "--root",
        &root,
        "--exec-db",
        &exec,
        "--action",
        "status",
    ]);
    assert_eq!(code, 0, "{err}");
    let workflow: Value = serde_json::from_str(&out).expect("json");
    assert_eq!(workflow["ai_review"], json!(true), "{out}");
    assert!(
        workflow["recipes"]
            .as_array()
            .expect("recipes")
            .iter()
            .any(|r| r["id"] == json!("pr-agent")),
        "the recipe list keeps the PR-Agent recipe: {out}"
    );
}

#[test]
fn a_registry_of_the_wrong_shape_is_refused_in_serdes_own_words() {
    // Round 5 cut serde's quotation out of the launch build's errors on the
    // two operator files. This build says what serde says, quotation and all.
    let f = Fixture::new();
    f.registry_with(json!({"recipes": "made-up-tool"}));
    let (code, out, err) = f.review("preview", "full", "");
    assert_ne!(code, 0, "{out}");
    assert!(
        err.contains("invalid type: string \"made-up-tool\", expected a sequence"),
        "{err}"
    );
    f.registry_with(json!({"made-up-tool": true}));
    let (code, out, err) = f.review("preview", "full", "");
    assert_ne!(code, 0, "{out}");
    assert!(err.contains("unknown field `made-up-tool`"), "{err}");

    // The tool registry: the setup text every recipe serves quotes the field.
    std::fs::write(
        &f.registry,
        serde_json::to_vec(
            &json!({"schema": "thruline.integration-runtimes.v1", "tools": {}, "made-up-tool": 1}),
        )
        .expect("json"),
    )
    .expect("registry");
    let root = f.root.display().to_string();
    let exec = f.exec.display().to_string();
    let (code, out, err) = f.run(&[
        "integration",
        "workflow",
        "--root",
        &root,
        "--exec-db",
        &exec,
        "--action",
        "status",
    ]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("unknown field `made-up-tool`"), "{out}");
}

#[test]
fn evaluate_names_the_packs_it_was_given_as_it_always_did() {
    // AI-REVIEW-OUT-OF-LAUNCH-R6. The launch build refuses an evaluation that
    // names a pack it does not ship. This build ships every pack, so the same
    // two files are evaluated and every pack is named, as before round 6:
    // green at the commit before the fix and after it.
    let f = Fixture::new();
    let dir = f._temp.path().join("inputs");
    std::fs::create_dir_all(&dir).expect("inputs");
    let packs = ["qodo-merge", "coderabbit9", "PR-Agent", "pr-agent"];
    let cases = json!({
        "schema": "thruline.integration-qualification-cases.v2",
        "cases": packs.iter().enumerate().map(|(i, pack)| json!({"id": format!("c{i}"), "pack": pack, "label": "clean"})).collect::<Vec<_>>(),
    })
    .to_string();
    std::fs::write(dir.join("cases.json"), &cases).expect("cases");
    let results = json!({
        "schema": "thruline.upstream-qualification.v1",
        "manifest_sha256": thruline_core::chain::sha256_hex(cases.as_bytes()),
        "cases": packs.iter().enumerate().map(|(i, pack)| json!({"id": format!("c{i}"), "pack": pack, "actual": "passed"})).collect::<Vec<_>>(),
    })
    .to_string();
    std::fs::write(dir.join("results.json"), results).expect("results");
    let (code, out, err) = f.run(&[
        "integration",
        "evaluate",
        "--cases",
        &dir.join("cases.json").display().to_string(),
        "--results",
        &dir.join("results.json").display().to_string(),
    ]);
    assert_eq!(code, 0, "{out}{err}");
    says_nothing_launch_only(&format!("{out}{err}"));
    let evaluation: Value = serde_json::from_str(&out).expect("json");
    assert_eq!(evaluation["total"]["cases"], json!(4), "{out}");
    for pack in packs {
        assert_eq!(
            evaluation["by_adapter"][pack]["cases"],
            json!(1),
            "{pack:?} is a by_adapter key: {out}"
        );
    }
    let named: Vec<&str> = evaluation["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .filter_map(|c| c["pack"].as_str())
        .collect();
    assert_eq!(named, packs, "every case names its pack: {out}");
}
