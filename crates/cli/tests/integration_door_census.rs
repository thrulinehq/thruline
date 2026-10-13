//! AI-REVIEW-OUT-OF-LAUNCH-R6 — the door census, the half that runs the drives.
//!
//! `tests/support/integration_doors.json` classifies every `thruline
//! integration` subcommand and argument and every action of
//! `POST /api/integrations/action`, and names, for each door that reads an id,
//! a pack or a recipe from an operator's flag, file or request body, the drive
//! that hands it an id the build does not ship. The two unit tests
//! (`integration_cmd/door_census.rs`, and the route's own) take the doors from
//! the clap definitions and the route table and fail on any that table does not
//! classify, so a door cannot be missed by a list nobody kept: round 5's grep
//! missed `integration evaluate`, which printed an operator-supplied pack as a
//! key and as a field.
//!
//! This file runs every drive against the compiled launch build with every id
//! the rounds attack it with and asserts, for each, that the id is REFUSED or
//! COUNTED and is never printed: not in stdout, not in stderr, not in the body
//! of a response, and not in any file the door wrote.
//!
//! The launch build only: the `ai-review` build names what an operator gives it
//! and `ai_review_feature_unchanged.rs` pins that. Every fixture is synthetic.
#![cfg(all(target_os = "macos", not(feature = "ai-review")))]

use serde_json::{json, Value};
#[path = "support/review_fixture.rs"]
mod fixture;
use fixture::{Fixture, Server, NOT_A_SPELLING, ROUND_4, ROUND_5};

const TABLE: &str = include_str!("support/integration_doors.json");

/// Round 6: the adversary's own ids, and the withheld id in its own spelling.
/// (`qodo-merge` and `PR-Agent` are round 5's and round 4's; they are here
/// because the evaluation repro named these three.)
const ROUND_6: &[&str] = &["qodo-merge", "coderabbit9", "PR-Agent", "pr-agent"];

/// What a request for a check this build does not run is answered with.
const CHECK_NOT_RUN: &str = "This build does not run that check.";

/// What a selection this build does not run becomes: a count, never the text.
const SELECTION_ONE: &str =
    "1 registered selection is not one this build runs, so it was left out of this review.";

/// Sentences this build says in fixed words whatever it was asked, and which
/// happen to contain the display name of the withheld review. A door that says
/// one is not echoing an id that is spelled the same way.
///   * round 1's refusal, ruled to stay (`AI_REVIEW_NOT_IN_BUILD`);
///   * T-4-1a's technical detail for a Change-bound review, which stays for
///     any path that can still reach the review.
const FIXED_SENTENCES: &[&str] = &[
    "AI review is not provisioned in this build of ThruLine: it was built without the ai-review feature, so no PR-Agent review runs",
    "Change-bound review has no PR-Agent plan",
];

fn every_id() -> Vec<&'static str> {
    let mut ids: Vec<&'static str> = ROUND_4
        .iter()
        .chain(ROUND_5)
        .chain(NOT_A_SPELLING)
        .chain(ROUND_6)
        .copied()
        .collect();
    let mut seen = std::collections::BTreeSet::new();
    ids.retain(|id| seen.insert(*id));
    ids
}

fn table() -> Value {
    serde_json::from_str(TABLE).expect("the door census table is JSON")
}

fn strings_in(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) => out.push(s.clone()),
        Value::Array(a) => a.iter().for_each(|x| strings_in(x, out)),
        Value::Object(m) => {
            for (k, x) in m {
                out.push(k.clone());
                strings_in(x, out);
            }
        }
        _ => {}
    }
}

fn without_fixed_sentences(text: &str) -> String {
    FIXED_SENTENCES
        .iter()
        .fold(text.to_owned(), |t, sentence| t.replace(sentence, ""))
}

/// Where in what a door printed the id is, if it is: the raw text, or any
/// string of a JSON document in it (so an escaped spelling is found as well).
fn echoed_in(id: &str, texts: &[&str]) -> Option<String> {
    for text in texts {
        let text = without_fixed_sentences(text);
        if text.contains(id) {
            return Some("in the text".into());
        }
        if let Ok(doc) = serde_json::from_str::<Value>(&text) {
            let mut all = Vec::new();
            strings_in(&doc, &mut all);
            if all.iter().any(|s| s.contains(id)) {
                return Some("in a string of the JSON".into());
            }
        }
    }
    None
}

/* ── the operator's files, built around an id ──────────────────────────── */

struct Inputs {
    dir: std::path::PathBuf,
    plan: Value,
}

impl Inputs {
    fn new(f: &Fixture) -> Self {
        let dir = f._temp.path().join("inputs");
        std::fs::create_dir(&dir).expect("inputs");
        let root = f.root.display().to_string();
        let (code, out, err) = f.run(&[
            "integration",
            "plan",
            "ast-grep",
            "--root",
            &root,
            "--executable",
            "/bin/echo",
        ]);
        assert_eq!(code, 0, "{err}");
        Self {
            dir,
            plan: serde_json::from_str(&out).expect("plan json"),
        }
    }

    fn path(&self, name: &str) -> String {
        self.dir.join(name).display().to_string()
    }

    /// Write every file a drive may name, each carrying `id` where it names a
    /// pack: `plan.json` as `integration plan` made it, `plan-nosupport.json`
    /// without the frozen support declaration older plans omit (so the pack is
    /// decided on its own), the evaluation's two files with the id as a case's
    /// pack beside a shipped one, and a report.
    fn write(&self, id: &str) {
        let mut plan = self.plan.clone();
        plan["pack"] = json!(id);
        std::fs::write(self.dir.join("plan.json"), plan.to_string()).expect("plan");
        plan.as_object_mut().expect("plan object").remove("support");
        std::fs::write(self.dir.join("plan-nosupport.json"), plan.to_string()).expect("plan");
        let cases = json!({
            "schema": "thruline.integration-qualification-cases.v2",
            "cases": [
                {"id": "c1", "pack": "ast-grep", "label": "clean"},
                {"id": "c2", "pack": id, "label": "failure"},
            ],
        })
        .to_string();
        std::fs::write(self.dir.join("cases.json"), &cases).expect("cases");
        let results = json!({
            "schema": "thruline.upstream-qualification.v1",
            "manifest_sha256": thruline_core::chain::sha256_hex(cases.as_bytes()),
            "cases": [
                {"id": "c1", "pack": "ast-grep", "actual": "passed"},
                {"id": "c2", "pack": id, "actual": "failed"},
            ],
        })
        .to_string();
        std::fs::write(self.dir.join("results.json"), results).expect("results");
        std::fs::write(self.dir.join("report.json"), "[]").expect("report");
    }
}

fn substitute(
    token: &str,
    f: &Fixture,
    inputs: &Inputs,
    id: &str,
    action: &str,
    plan: &str,
) -> String {
    match token {
        "{id}" => id.to_owned(),
        "{root}" => f.root.display().to_string(),
        "{exec}" => f.exec.display().to_string(),
        "{plan}" => inputs.path(plan),
        "{cases}" => inputs.path("cases.json"),
        "{results}" => inputs.path("results.json"),
        "{report}" => inputs.path("report.json"),
        "{action}" => action.to_owned(),
        other => {
            assert!(!other.starts_with('{'), "the driver does not know {other}");
            other.to_owned()
        }
    }
}

/* ── the command line ──────────────────────────────────────────────────── */

#[test]
fn every_integration_door_that_reads_an_id_refuses_or_counts_one_the_build_does_not_ship_and_prints_none_of_it(
) {
    let f = Fixture::new();
    let inputs = Inputs::new(&f);
    let table = table();
    let mut failures = Vec::new();
    let mut runs = 0usize;

    for (name, door) in table["cli"].as_object().expect("cli") {
        for drive in door["drives"].as_array().expect("drives") {
            let outcome = drive["outcome"].as_str().expect("outcome");
            let via: Vec<&str> = match &drive["via"] {
                Value::String(one) => vec![one.as_str()],
                Value::Array(many) => many.iter().filter_map(Value::as_str).collect(),
                other => panic!("via: {other}"),
            };
            let registers = via.iter().any(|v| v.starts_with("env:"));
            let actions: Vec<&str> = match drive.get("for_action") {
                Some(list) => list
                    .as_array()
                    .expect("for_action")
                    .iter()
                    .filter_map(Value::as_str)
                    .collect(),
                None => vec![""],
            };
            let template: Vec<&str> = drive["argv"]
                .as_array()
                .expect("argv")
                .iter()
                .filter_map(Value::as_str)
                .collect();
            let plans: &[&str] = if template.contains(&"{plan}") {
                &["plan.json", "plan-nosupport.json"]
            } else {
                &["plan.json"]
            };
            for action in &actions {
                for plan in plans {
                    for id in every_id() {
                        inputs.write(id);
                        if registers {
                            f.select(&["ast-grep", id]);
                        } else {
                            f.registry_with(json!({}));
                        }
                        let mut argv = vec!["integration".to_owned(), name.clone()];
                        argv.extend(
                            template
                                .iter()
                                .map(|t| substitute(t, &f, &inputs, id, action, plan)),
                        );
                        let argv: Vec<&str> = argv.iter().map(String::as_str).collect();
                        let door_name = format!("integration {name} via {via:?} {action}");
                        let before = f.written_hits(id, FIXED_SENTENCES);
                        let (code, out, err) = f.run(&argv);
                        runs += 1;

                        match outcome {
                            "refused" if code == 0 => {
                                failures.push(format!("{door_name} {id:?}: exits 0, not a refusal"))
                            }
                            "counted" => {
                                if code != 0 || !out.contains(SELECTION_ONE) {
                                    failures.push(format!(
                                        "{door_name} {id:?}: not counted (exit {code}): {out}{err}"
                                    ));
                                }
                            }
                            "unused" if code != 0 => failures.push(format!(
                                "{door_name} {id:?}: exits {code}, and it does not use the id: {err}"
                            )),
                            _ => {}
                        }
                        if let Some(at) = echoed_in(id, &[&out, &err]) {
                            failures
                                .push(format!("{door_name}: {id:?} is printed {at}: {out}{err}"));
                        }
                        let after = f.written_hits(id, FIXED_SENTENCES);
                        if after != before {
                            failures.push(format!(
                                "{door_name}: {id:?} is written into a file ({before} -> {after} hits)"
                            ));
                        }
                    }
                }
            }
        }
    }
    assert!(
        runs > 300,
        "the census drove {runs} runs: too few to be the census"
    );
    assert!(
        failures.is_empty(),
        "{} of {runs} runs failed:\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}

#[test]
fn the_same_doors_still_work_for_an_id_the_build_ships() {
    // The refusals above are the allow-list's, not a door that refuses
    // everything: the shipped id passes the same doors.
    let f = Fixture::new();
    let inputs = Inputs::new(&f);
    let root = f.root.display().to_string();

    let (code, out, err) = f.run(&[
        "integration",
        "plan",
        "ast-grep",
        "--root",
        &root,
        "--executable",
        "/bin/echo",
    ]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("ast-grep"), "{out}");

    inputs.write("ast-grep");
    let (code, out, err) = f.run(&[
        "integration",
        "decode",
        "ast-grep",
        &inputs.path("report.json"),
    ]);
    assert_eq!(code, 0, "{out}{err}");

    // An evaluation of shipped packs is made, and names them.
    let (code, out, err) = f.run(&[
        "integration",
        "evaluate",
        "--cases",
        &inputs.path("cases.json"),
        "--results",
        &inputs.path("results.json"),
    ]);
    assert_eq!(code, 0, "{out}{err}");
    let evaluation: Value = serde_json::from_str(&out).expect("json");
    assert_eq!(evaluation["total"]["cases"], json!(2), "{out}");
    assert_eq!(
        evaluation["by_adapter"]["ast-grep"]["cases"],
        json!(2),
        "{out}"
    );
    let packs: Vec<&str> = evaluation["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .filter_map(|c| c["pack"].as_str())
        .collect();
    assert_eq!(packs, ["ast-grep", "ast-grep"], "{out}");
}

/* ── the route ─────────────────────────────────────────────────────────── */

#[test]
fn every_action_of_the_hq_route_refuses_an_id_the_build_does_not_ship_and_prints_none_of_it() {
    let f = Fixture::new();
    let server = Server::start(&f);
    let table = table();
    let route = &table["route"];
    let actions: Vec<&str> = route["actions"]
        .as_array()
        .expect("actions")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let post =
        |body: Value| server.request("POST", "/api/integrations/action", Some(&body.to_string()));
    let mut failures = Vec::new();
    let mut runs = 0usize;

    for drive in route["drives"].as_array().expect("drives") {
        let via = drive["via"].as_str().expect("via");
        for id in every_id() {
            let bodies: Vec<(String, Value)> = match via {
                "recipe" => actions
                    .iter()
                    .map(|action| {
                        (
                            format!("recipe at {action}"),
                            json!({"session": "review-t", "action": action, "recipe": id, "digest": ""}),
                        )
                    })
                    .collect(),
                "action" => vec![(
                    "action".into(),
                    json!({"session": "review-t", "action": id, "recipe": "full", "digest": ""}),
                )],
                "session" => vec![(
                    "session".into(),
                    json!({"session": id, "action": "review-status", "recipe": "full", "digest": ""}),
                )],
                other => panic!("the driver does not know the route field {other}"),
            };
            for (what, body) in bodies {
                let before = f.written_hits(id, FIXED_SENTENCES);
                let (status, text) = post(body);
                runs += 1;
                if status < 400 {
                    failures.push(format!("{what} {id:?}: answered {status}: {text}"));
                }
                if via == "recipe" {
                    // The gate: refused before anything is spawned, in fixed
                    // words, or in round 1's sentence for the withheld review.
                    let fixed = serde_json::from_str::<Value>(&text)
                        .is_ok_and(|v| v == json!({"error": CHECK_NOT_RUN}));
                    if status != 400 || !(fixed || text.contains("AI review is not provisioned")) {
                        failures.push(format!(
                            "{what} {id:?}: not the gate's answer: {status} {text}"
                        ));
                    }
                }
                if let Some(at) = echoed_in(id, &[&text]) {
                    failures.push(format!("{what}: {id:?} is printed {at}: {text}"));
                }
                let after = f.written_hits(id, FIXED_SENTENCES);
                if after != before {
                    failures.push(format!("{what}: {id:?} is written into a file"));
                }
            }
        }
    }
    assert!(
        runs > 500,
        "the census drove {runs} requests: too few to be the census"
    );
    assert!(
        failures.is_empty(),
        "{} of {runs} requests failed:\n  {}",
        failures.len(),
        failures.join("\n  ")
    );

    // And the route still admits what the build ships.
    for (action, recipe) in [("review-preview", "full"), ("preview", "ast-grep")] {
        let (status, text) =
            post(json!({"session": "review-t", "action": action, "recipe": recipe, "digest": ""}));
        assert_ne!(status, 400, "{action} {recipe}: {text}");
    }
}
