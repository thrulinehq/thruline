use super::{parse_json, DecodeError, Observation, Report};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn invalid(message: &str) -> DecodeError {
    DecodeError::Invalid(message.into())
}

pub fn decode(bytes: &[u8]) -> Result<Report, DecodeError> {
    let value = parse_json(bytes)?;
    if !matches!(
        value["schema"].as_str(),
        Some("thruline.clippy-capture.v1" | "thruline.clippy-capture.v2")
    ) || value["version"] != "0.1.97"
    {
        return Err(invalid("unrecognized Clippy capture contract"));
    }
    let v2 = value["schema"] == "thruline.clippy-capture.v2";
    if v2
        && (value["configuration"]["schema"] != "thruline.clippy.v2"
            || value["configuration"]["lints"] != json!(["clippy::unnecessary_map_or"])
            || !matches!(
                value["configuration"]["suppression"].as_str(),
                Some("force-warn" | "respect")
            ))
    {
        return Err(invalid("unexpected selected Clippy MSRV policy"));
    }
    let raw = value["native_stdout"]
        .as_str()
        .ok_or_else(|| invalid("missing Cargo JSONL"))?;
    let events = value["events"]
        .as_array()
        .ok_or_else(|| invalid("missing Cargo events"))?;
    let native: Vec<Value> = raw
        .lines()
        .filter_map(|line| parse_json(line.as_bytes()).ok())
        .collect();
    if &native != events {
        return Err(invalid("captured Cargo events differ from native JSONL"));
    }
    let root = value["source_root"]
        .as_str()
        .ok_or_else(|| invalid("missing Rust source root"))?;
    let artifacts: Vec<&Value> = events
        .iter()
        .filter(|row| row["reason"] == "compiler-artifact")
        .collect();
    let mut report = Report {
        format: "clippy-json-v1".into(),
        observations: vec![],
        facts: BTreeMap::new(),
        complete: value["exit_code"] == 0
            && value["metadata_exit"] == 0
            && value["original_unchanged"] == true
            && !artifacts.is_empty()
            && events
                .last()
                .is_some_and(|row| row["reason"] == "build-finished" && row["success"] == true)
            && value["parse_error_lines"]
                .as_array()
                .is_some_and(Vec::is_empty)
            && value["symlinks_not_followed"]
                .as_array()
                .is_some_and(Vec::is_empty),
        limitations: vec![],
    };
    let mut diagnostics = vec![];
    for (index, event) in events.iter().enumerate() {
        if event["reason"] != "compiler-message" {
            continue;
        }
        let message = &event["message"];
        let level = message["level"]
            .as_str()
            .ok_or_else(|| invalid("missing compiler severity"))?;
        if level == "error" {
            report.complete = false;
        }
        diagnostics.push(event.clone());
        let rule = message["code"]["code"]
            .as_str()
            .unwrap_or("rustc-diagnostic");
        if !rule.starts_with("clippy::") && level != "error" {
            continue;
        }
        let spans = message["spans"]
            .as_array()
            .ok_or_else(|| invalid("missing diagnostic spans"))?;
        let primary = spans.iter().find(|span| span["is_primary"] == true);
        let path = primary
            .and_then(|span| span["file_name"].as_str())
            .map(|p| p.strip_prefix(&format!("{root}/")).unwrap_or(p).to_owned());
        let line = primary.and_then(|span| span["line_start"].as_u64());
        if line == Some(0) {
            return Err(invalid("invalid compiler source line"));
        }
        report.observations.push(Observation {
            rule: rule.into(),
            message: message["message"]
                .as_str()
                .ok_or_else(|| invalid("missing compiler message"))?
                .into(),
            path,
            line,
            pointer: format!("/events/{index}/message"),
        });
    }
    report
        .facts
        .insert("finding_count".into(), json!(report.observations.len()));
    report
        .facts
        .insert("native_diagnostics".into(), json!(diagnostics));
    report
        .facts
        .insert("compiled_targets".into(), json!(artifacts));
    for key in [
        "configuration",
        "metadata",
        "toolchain",
        "discovered_source_hashes",
        "native_stderr",
        "artifacts",
        "exclusions",
    ] {
        report.facts.insert(key.into(), value[key].clone());
    }
    if v2 {
        if value["policy_unchanged"] != true
            || value["msrv_scope"]["selected_msrv"] != value["configuration"]["msrv"]
            || value["msrv_scope"]["conflicts"]
                .as_array()
                .is_none_or(|rows| !rows.is_empty())
        {
            report.complete = false;
        }
        for key in ["policy_files", "policy_unchanged", "msrv_scope"] {
            report.facts.insert(key.into(), value[key].clone());
        }
        report.limitations.push("The explicit mode selects only unnecessary_map_or at the captured MSRV. Respect mode honors native local suppression; force-warn explicitly overrides allow attributes. Source MSRV attributes may select lexical scopes. This modern compiler run does not prove compilation by a historical compiler.".into());
    }
    if !report.complete {
        report.limitations.push("The selected Rust build did not establish complete successful compilation; inspect errors, skipped/symlink inputs and target inventory.".into());
    }
    if !v2 {
        report.limitations.push("Compiler-backed coverage includes the selected targets and enabled feature configuration only. Files or cfg branches not compiled in this configuration remain unexamined. This recipe force-enables only clippy::needless_return; it does not enable the restriction group or claim all Clippy lints.".into());
    }
    Ok(report)
}

#[cfg(test)]
mod selected_msrv_tests {
    use super::*;
    #[test]
    fn native_msrv_and_respected_suppression_are_disclosed() {
        let high = decode(include_bytes!(
            "../../tests/fixtures/native-modes/clippy-high.json"
        ))
        .unwrap();
        assert!(high.complete);
        assert_eq!(high.observations.len(), 1);
        assert_eq!(high.facts["msrv_scope"]["selected_msrv"], "1.70.0");
        let respect = decode(include_bytes!(
            "../../tests/fixtures/native-modes/clippy-respect.json"
        ))
        .unwrap();
        assert!(respect.complete);
        assert!(respect.observations.is_empty());
        assert_eq!(respect.facts["msrv_scope"]["suppression"], "respect");
    }
    #[test]
    fn native_config_conflict_and_compiler_error_are_incomplete() {
        for bytes in [
            include_bytes!("../../tests/fixtures/native-modes/clippy-conflict.json").as_slice(),
            include_bytes!("../../tests/fixtures/native-modes/clippy-invalid.json").as_slice(),
        ] {
            let report = decode(bytes).unwrap();
            assert!(!report.complete);
            assert!(!report.facts["native_diagnostics"]
                .as_array()
                .unwrap()
                .is_empty());
        }
    }
}
