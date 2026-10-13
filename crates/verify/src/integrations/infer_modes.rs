//! Checker facts keep their native meaning: purity is positive information,
//! polynomial degree has a native encoding, and missing models remain gaps.
use super::{parse_json, DecodeError, Observation, Report};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
fn bad(s: &str) -> DecodeError {
    DecodeError::Invalid(s.into())
}
fn array<'a>(v: &'a Value, key: &str) -> Result<&'a Vec<Value>, DecodeError> {
    v[key]
        .as_array()
        .ok_or_else(|| bad(&format!("missing Infer mode array {key}")))
}
fn gap(r: &mut Report, s: &str) {
    r.complete = false;
    r.limitations.push(s.into());
}
fn digest(v: &Value) -> bool {
    v.as_str().is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    })
}
fn reference(v: &Value) -> bool {
    v["path"].as_str().is_some_and(|s| !s.is_empty())
        && digest(&v["sha256"])
        && v["bytes"].as_u64().is_some()
}
fn issue(r: &mut Report, row: &Value, pointer: String, source: &Value) -> Result<(), DecodeError> {
    let rule = row["bug_type"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| bad("native Infer issue lacks type"))?;
    let message = row["qualifier"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| bad("native Infer issue lacks description"))?;
    let file = row["file"]
        .as_str()
        .ok_or_else(|| bad("native Infer issue lacks source"))?;
    let stage = source["isolated_source"]
        .as_str()
        .ok_or_else(|| bad("isolated Infer source missing"))?;
    let path = file.strip_prefix(&format!("{stage}/")).unwrap_or(file);
    if !array(source, "sources")?.iter().any(|v| v == path) || row["bug_trace"].as_array().is_none()
    {
        return Err(bad(
            "native Infer issue is outside selected scope or lacks trace",
        ));
    }
    let line = row["line"]
        .as_u64()
        .filter(|n| *n > 0)
        .ok_or_else(|| bad("native Infer source line invalid"))?;
    r.observations.push(Observation {
        rule: rule.into(),
        message: message.into(),
        path: Some(path.into()),
        line: Some(line),
        pointer,
    });
    Ok(())
}
pub fn decode(bytes: &[u8]) -> Result<Report, DecodeError> {
    let v = parse_json(bytes)?;
    if v["schema"] != "thruline.infer-mode-capture.v1" || v["version"] != "1.3.0" {
        return Err(bad("unknown Infer mode capture"));
    }
    let mode = v["mode"]
        .as_str()
        .filter(|s| {
            [
                "racerd",
                "cost",
                "purity",
                "impurity",
                "loop-hoisting",
                "pulse",
                "taint",
                "rust-ir",
                "erlang",
                "python",
            ]
            .contains(s)
        })
        .ok_or_else(|| bad("unsupported Infer mode"))?;
    if v["configuration"]["schema"] != "thruline.infer-modes.v1"
        || v["configuration"]["mode"] != mode
        || !digest(&v["configuration_sha256"])
        || !reference(&v["runtime_manifest"])
        || !reference(&v["native_executable"])
    {
        return Err(bad(
            "missing accepted Infer configuration or runtime identity",
        ));
    }
    let mut r = Report {
        format: "infer-json-v1".into(),
        observations: vec![],
        facts: BTreeMap::new(),
        complete: v["complete"] == true && v["original_unchanged"] == true,
        limitations: vec![],
    };
    if !array(&v, "errors")?.is_empty() {
        gap(&mut r,"Native Infer execution or bounded artifact capture failed; inspect retained commands and streams.");
    }
    let stages = v["stages"]
        .as_object()
        .ok_or_else(|| bad("native stages missing"))?;
    let current = stages
        .get("current")
        .ok_or_else(|| bad("current capture missing"))?;
    if current["sources"] != v["configuration"]["sources"] {
        return Err(bad(
            "native source scope differs from selected configuration",
        ));
    }
    let java = !["rust-ir", "python", "erlang"].contains(&mode);
    for (name, stage) in stages {
        for phase in if java {
            vec!["analysis", "summary_json", "summary_text"]
        } else {
            vec!["analysis"]
        } {
            let label = format!("{name}-{phase}");
            let rows: Vec<_> = array(&v, "commands")?
                .iter()
                .filter(|c| c["name"] == label)
                .collect();
            if rows.len() != 1 {
                gap(
                    &mut r,
                    "A required native analysis or summary phase is missing or repeated.",
                );
            } else if phase == "analysis" && *rows[0] != stage["command"] {
                return Err(bad("native phase differs from its recorded command"));
            }
        }
        if mode == "cost"
            && stage["costs"].as_array().is_some_and(|rows| {
                rows.iter().any(|c| {
                    c["exec_cost"]["degree"].as_u64().is_none()
                        || c["exec_cost"]["polynomial_version"] != 13
                        || c["exec_cost"]["hum"]["hum_degree"] == "Top"
                })
            })
        {
            gap(&mut r,"Current or baseline native cost has an unknown bound or unsupported polynomial encoding.");
        }
        for key in ["diagnostics", "costs"] {
            if let Some(raw) = stage["native_reports"][key].as_str() {
                if parse_json(raw.as_bytes())? != stage[key] {
                    return Err(bad(
                        "normalized Infer data differs from its native report bytes",
                    ));
                }
            } else if !stage[key].is_null() {
                gap(
                    &mut r,
                    "Native report body is missing; normalized data alone is insufficient.",
                );
            }
        }
        let selected: BTreeSet<_> = array(stage, "sources")?
            .iter()
            .map(|v| v.as_str().ok_or_else(|| bad("selected source not text")))
            .collect::<Result<_, _>>()?;
        if selected.is_empty() {
            gap(&mut r, "No selected source files were captured.");
        }
        let mut captured = BTreeSet::new();
        let mut fresh = true;
        for row in array(stage, "captured_files")? {
            let encoded = row["native_encoding"]
                .as_str()
                .ok_or_else(|| bad("native source encoding missing"))?;
            let path = if let Some(p) = encoded.strip_prefix('R') {
                Some(p)
            } else if let Some(p) = encoded.strip_prefix('A') {
                p.strip_prefix(&format!(
                    "{}/",
                    stage["isolated_source"].as_str().unwrap_or("")
                ))
            } else {
                None
            };
            if let Some(path) = path {
                captured.insert(path);
            } else if java {
                fresh = false;
            }
            fresh &= row["freshly_captured"] == 1;
        }
        if stage["command"]["exit_code"] != 0
            || !stage["capture_error"].is_null()
            || !fresh
            || (java && selected != captured)
            || stage["diagnostics"].as_array().is_none()
        {
            gap(&mut r,&format!("{name}: Infer did not establish fresh native capture for every explicitly selected source."));
        }
        if java
            && (stage["summary_json"].as_array().is_none_or(Vec::is_empty)
                || stage["summary_text"].as_str().is_none_or(str::is_empty))
        {
            gap(
                &mut r,
                &format!("{name}: selected native procedure summaries were not captured."),
            );
        }
    }
    for command in array(&v, "commands")? {
        if !reference(&command["stdout"]) || !reference(&command["stderr"]) {
            return Err(bad("native command stream identity missing"));
        }
        if command["exit_code"] != 0
            || command["timed_out"] != false
            || command["stream_limit"] != false
        {
            gap(
                &mut r,
                "A native Infer phase did not finish within its declared limits.",
            );
        }
    }
    let diagnostics = current["diagnostics"].as_array();
    let mut positive = vec![];
    let mut side_effects = vec![];
    if let Some(rows) = diagnostics {
        for (i, row) in rows.iter().enumerate() {
            match row["bug_type"].as_str() {
                Some("PURE_FUNCTION") => positive.push(row.clone()),
                Some("IMPURE_FUNCTION") => {
                    side_effects.push(row.clone());
                    if v["configuration"]["require_pure"] == true {
                        issue(
                            &mut r,
                            row,
                            format!("/stages/current/diagnostics/{i}"),
                            current,
                        )?;
                    }
                }
                _ => issue(
                    &mut r,
                    row,
                    format!("/stages/current/diagnostics/{i}"),
                    current,
                )?,
            }
        }
    }
    let mut costs = vec![];
    if let Some(rows) = current["costs"].as_array() {
        for (i, row) in rows.iter().enumerate() {
            let cost = &row["exec_cost"];
            let degree = cost["degree"].as_u64();
            let known = degree.is_some()
                && cost["polynomial_version"] == 13
                && cost["hum"]["hum_degree"] != "Top";
            costs.push(json!({"native":row,"pointer":format!("/stages/current/costs/{i}"),"known":known,"linear_degree":degree.map(|n|n/100),"log_degree":degree.map(|n|n%100)}));
            if ["cost", "loop-hoisting"].contains(&mode) && !known {
                gap(&mut r,"A native cost is Top or lacks a supported degree. This is an unknown bound, not a clean complexity result.");
            }
        }
    }
    if mode == "cost" && costs.is_empty() {
        gap(&mut r, "No native cost records were produced.");
    }
    if !array(&v["configuration"], "baseline_sources")?.is_empty() {
        if v["differential"]["introduced"].as_array().is_none()
            || v["differential"]["fixed"].as_array().is_none()
        {
            gap(&mut r, "Native cost differential is missing.");
        } else {
            for (i, row) in array(&v["differential"], "introduced")?.iter().enumerate() {
                issue(
                    &mut r,
                    row,
                    format!("/differential/introduced/{i}"),
                    current,
                )?;
            }
        }
    }
    let summary = current["summary_text"].as_str().unwrap_or("");
    let any_thread = summary.matches("Threads: AnyThread").count();
    let no_thread = summary.matches("Threads: NoThread").count();
    if mode == "racerd" && any_thread == 0 {
        gap(&mut r,"RacerD did not establish a concurrent context for the selected procedures. NoThread is not proof of race freedom.");
    }
    let skipped = array(&v, "pulse_skipped_calls")?;
    if ["pulse", "taint"].contains(&mode) && !skipped.is_empty() {
        gap(
            &mut r,
            "Native Pulse summaries contain skipped external calls. Their behavior is a model gap.",
        );
    }
    let models = array(&v, "taint_model_matches")?;
    if mode == "taint" && (models.is_empty() || models.iter().any(|m| m["matched"] != true)) {
        gap(
            &mut r,
            "A selected taint source or sink model does not match a captured defined procedure.",
        );
    }
    if ["rust-ir", "erlang", "python"].contains(&mode) {
        gap(&mut r,match mode{"rust-ir"=>"Experimental Rust Pulse analyzed selected existing ULLBC only. Source-to-IR compilation and equivalence are not established by this recipe.","erlang"=>"Erlang Pulse is experimental. The prepared runtime has no Erlang compiler/escript closure; the actual capture failure is retained.",_=>"Pinned Pulse documentation lists Python unsupported, and this Infer binary has Python analyzers disabled. No Python analysis result is available."});
    }
    r.facts.insert(
        "finding_count".into(),
        if diagnostics.is_some() {
            json!(r.observations.len())
        } else {
            Value::Null
        },
    );
    r.facts
        .insert("native_diagnostics".into(), current["diagnostics"].clone());
    r.facts
        .insert("native_artifacts".into(), v["artifacts"].clone());
    r.facts.insert("infer_mode".into(),json!({"mode":mode,"version":v["version"],"configuration":v["configuration"],"configuration_sha256":v["configuration_sha256"],"source_root":v["source_root"],"source_hashes":v["source_hashes"],"runtime_manifest":v["runtime_manifest"],"native_executable":v["native_executable"],"deprecated":(["purity","impurity","loop-hoisting"].contains(&mode)),"experimental":(["rust-ir","erlang"].contains(&mode)),"commands":v["commands"],"stages":v["stages"],"costs":costs,"differential":v["differential"],"pure_functions":positive,"side_effect_facts":side_effects,"concurrency_context":{"any_thread":any_thread,"no_thread":no_thread},"pulse_skipped_calls":skipped,"pulse_preconditions":v["pulse_preconditions"],"taint_model_matches":models,"errors":v["errors"],"original_unchanged":v["original_unchanged"]}));
    r.limitations.push("These observations cover the explicitly selected Infer checker, Java17 sources or experimental IR inputs. Native summaries can assume caller preconditions; absence of a manifest issue does not prove those preconditions. Cost is an abstract instruction polynomial, not measured execution time. Infer provides no automatic source repair for these modes; corrections require separate behavior evidence.".into());
    if ["purity", "impurity", "loop-hoisting"].contains(&mode) {
        r.limitations.push("Infer1.3.0 marks the selected checker deprecated; purity/impurity are experimental. A PURE_FUNCTION result is positive information, not a defect.".into());
    }
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(name: &str) -> Value {
        let bytes = match name {
            "regression" => include_str!("fixtures/infer-modes/costdiff-regression.json"),
            "top" => include_str!("fixtures/infer-modes/costdiff-unknown.json"),
            "external" => include_str!("fixtures/infer-modes/owned-external-gap.json"),
            "manifest" => include_str!("fixtures/infer-modes/owned-pulse-manifest.json"),
            "correct" => include_str!("fixtures/infer-modes/owned-pulse-correct.json"),
            "rust" => include_str!("fixtures/infer-modes/rust-defect.json"),
            _ => include_str!("fixtures/infer-modes/python-unavailable.json"),
        };
        parse_json(bytes.as_bytes()).unwrap()
    }
    fn report(v: &Value) -> Report {
        decode(&serde_json::to_vec(v).unwrap()).unwrap()
    }
    #[test]
    fn actual_native_cost_diff_and_encoded_degree_are_preserved() {
        let r = report(&fixture("regression"));
        assert!(r.complete);
        assert_eq!(r.observations[0].rule, "EXECUTION_TIME_COMPLEXITY_INCREASE");
        assert!(r.facts["infer_mode"]["costs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["linear_degree"] == 2 && c["native"]["exec_cost"]["degree"] == 200));
    }
    #[test]
    fn top_is_unknown_even_without_native_regression() {
        let r = report(&fixture("top"));
        assert!(!r.complete);
        assert!(r.observations.is_empty());
        assert!(r.limitations.iter().any(|s| s.contains("Top")));
    }
    #[test]
    fn skipped_external_summary_is_not_clean() {
        let r = report(&fixture("external"));
        assert!(!r.complete);
        assert!(r.observations.is_empty());
        assert!(!r.facts["infer_mode"]["pulse_skipped_calls"]
            .as_array()
            .unwrap()
            .is_empty());
    }
    #[test]
    fn native_manifest_and_guarded_control_remain_distinct() {
        let before = report(&fixture("manifest"));
        let after = report(&fixture("correct"));
        assert!(before.complete && after.complete);
        assert_eq!(before.observations.len(), 1);
        assert!(after.observations.is_empty());
        assert!(!after.facts["infer_mode"]["pulse_preconditions"]
            .as_array()
            .unwrap()
            .is_empty());
    }
    #[test]
    fn rust_ir_findings_do_not_qualify_source_compilation() {
        let r = report(&fixture("rust"));
        assert!(!r.complete);
        assert_eq!(r.observations.len(), 2);
        assert!(r.limitations.iter().any(|s| s.contains("Source-to-IR")));
    }
    #[test]
    fn python_native_build_failure_has_no_clean_empty_report() {
        let r = report(&fixture("python"));
        assert!(!r.complete);
        assert!(r.facts["finding_count"].is_null());
        assert!(r.limitations.iter().any(|s| s.contains("disabled")));
    }
    #[test]
    fn changed_normalized_diagnostic_is_rejected() {
        let mut v = fixture("manifest");
        v["stages"]["current"]["diagnostics"][0]["bug_type"] = json!("OTHER");
        assert!(decode(&serde_json::to_vec(&v).unwrap()).is_err());
    }
    #[test]
    fn capture_source_mismatch_is_rejected() {
        let mut v = fixture("manifest");
        v["configuration"]["sources"] = json!(["Other.java"]);
        assert!(decode(&serde_json::to_vec(&v).unwrap()).is_err());
    }
    #[test]
    fn unknown_polynomial_encoding_cannot_claim_a_bound() {
        let mut v = fixture("regression");
        v["stages"]["current"]["costs"][0]["exec_cost"]["polynomial_version"] = json!(99);
        v["stages"]["current"]["native_reports"]["costs"] =
            json!(serde_json::to_string(&v["stages"]["current"]["costs"]).unwrap());
        assert!(!report(&v).complete);
    }
}
