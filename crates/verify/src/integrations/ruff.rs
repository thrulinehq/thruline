//! Pinned Ruff native diagnostics plus explicit execution/scope observations.
//! Applicability is upstream metadata, never behavioral verification.
use super::{parse_json, DecodeError, Observation, Report};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

fn invalid(message: &str) -> DecodeError {
    DecodeError::Invalid(message.into())
}
fn strings(value: &Value) -> Result<BTreeSet<&str>, DecodeError> {
    value
        .as_array()
        .ok_or_else(|| invalid("missing Ruff source inventory"))?
        .iter()
        .map(|v| {
            v.as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| invalid("invalid Ruff source path"))
        })
        .collect()
}
pub fn decode(bytes: &[u8]) -> Result<Report, DecodeError> {
    let value = parse_json(bytes)?;
    if !matches!(
        value["schema"].as_str(),
        Some("thruline.ruff-capture.v1" | "thruline.ruff-capture.v2")
    ) || value["version"] != "0.16.7"
    {
        return Err(invalid("unrecognized Ruff capture version"));
    }
    let selected = value["schema"] == "thruline.ruff-capture.v2";
    if selected && value["capture_error"].is_string() {
        return Ok(Report { format:"ruff-json-v1".into(), observations:vec![], facts:BTreeMap::from([("native_capture_error".into(),value["capture_error"].clone())]), complete:false, limitations:vec!["The explicit Ruff selection could not run completely. Its setup/scope error is retained; no empty list clears this source.".into()] });
    }
    let mode = if selected {
        value["mode"]
            .as_str()
            .ok_or_else(|| invalid("missing explicit Ruff mode"))?
    } else {
        "lint"
    };
    let files = strings(&value["files"])?;
    let requested = strings(&value["requested_files"])?;
    let diagnostics = value["diagnostics"]
        .as_array()
        .ok_or_else(|| invalid("Ruff did not produce a diagnostic array"))?;
    let native = value["native_stdout"]
        .as_str()
        .ok_or_else(|| invalid("missing native Ruff output"))?;
    if parse_json(native.as_bytes())? != value["diagnostics"] {
        return Err(invalid(
            "Ruff diagnostics differ from retained native output",
        ));
    }
    let rules = strings(&value["rules"])?;
    let expected_rules = match mode {
        "lint" => BTreeSet::from(["F401", "F841"]),
        "imports" if selected => BTreeSet::from(["I001"]),
        "format" if selected => BTreeSet::from(["unformatted"]),
        _ => return Err(invalid("unsupported explicit Ruff mode")),
    };
    if rules != expected_rules {
        return Err(invalid("Ruff rule selection differs from captured recipe"));
    }
    let mut report = Report {
        format: "ruff-json-v1".into(),
        observations: vec![],
        facts: BTreeMap::new(),
        complete: true,
        limitations: vec!["Ruff fix applicability describes the upstream transformation. Independent behavior checks are required before resolving a requirement.".into()],
    };
    for (i, row) in diagnostics.iter().enumerate() {
        let rule = row["code"]
            .as_str()
            .ok_or_else(|| invalid("Ruff diagnostic has no rule code"))?;
        let path = row["filename"]
            .as_str()
            .ok_or_else(|| invalid("Ruff diagnostic has no filename"))?;
        let message = row["message"]
            .as_str()
            .ok_or_else(|| invalid("Ruff diagnostic has no message"))?;
        let line = row["location"]["row"]
            .as_u64()
            .filter(|n| *n > 0)
            .ok_or_else(|| invalid("Ruff diagnostic has no positive source line"))?;
        let parse_error = rule == "invalid-syntax";
        if !files.contains(path) || (!rules.contains(rule) && !parse_error) {
            return Err(invalid(
                "Ruff diagnostic lies outside the captured file/rule selection",
            ));
        }
        if parse_error && report.complete {
            report.complete = false;
            // The primary explanation must name the missing coverage before
            // general guidance about the applicability of proposed fixes.
            report.limitations.insert(0, "Ruff could not parse a selected source file. Its native syntax diagnostic is retained, but the selected F401/F841 rules cannot establish complete coverage for that file.".into());
        }
        for location in ["location", "end_location"] {
            if row[location]["row"].as_u64().is_none_or(|n| n == 0)
                || row[location]["column"].as_u64().is_none_or(|n| n == 0)
            {
                return Err(invalid("invalid Ruff diagnostic span"));
            }
        }
        if !row["fix"].is_null() {
            let fix = &row["fix"];
            if !matches!(
                fix["applicability"].as_str(),
                Some("safe" | "unsafe" | "display_only")
            ) || fix["edits"].as_array().is_none_or(Vec::is_empty)
            {
                return Err(invalid(
                    "unrecognized Ruff fix applicability or missing edits",
                ));
            }
        }
        report.observations.push(Observation {
            rule: rule.into(),
            message: message.into(),
            path: Some(path.into()),
            // Native notebook rows are cell-local, never notebook JSON lines.
            line: if selected && row["cell"].is_number() {
                None
            } else {
                Some(line)
            },
            pointer: format!("/diagnostics/{i}"),
        });
    }
    if selected {
        if value["original_unchanged"] != true {
            report.complete = false;
        }
        let locations = value["notebook_locations"]
            .as_array()
            .ok_or_else(|| invalid("missing notebook location accounting"))?;
        for (index, row) in diagnostics
            .iter()
            .enumerate()
            .filter(|(_, row)| row["cell"].is_number())
        {
            let location = locations
                .iter()
                .find(|item| item["diagnostic_index"] == index)
                .ok_or_else(|| invalid("native notebook diagnostic lacks cell identity"))?;
            let cell = row["cell"]
                .as_u64()
                .filter(|n| *n > 0)
                .ok_or_else(|| invalid("invalid native notebook cell index"))?;
            let cells = value["notebook_cells"][row["filename"].as_str().unwrap_or("")]
                .as_array()
                .ok_or_else(|| invalid("missing captured notebook cells"))?;
            let source_cell = cells
                .get((cell - 1) as usize)
                .ok_or_else(|| invalid("native cell outside notebook"))?;
            if source_cell["cell_type"] != "code"
                || location["filename"] != row["filename"]
                || location["cell_index"] != cell
                || location["cell_id"] != source_cell["id"]
                || location["cell_local_location"] != row["location"]
                || location["cell_local_end_location"] != row["end_location"]
                || !location["notebook_json_line"].is_null()
            {
                return Err(invalid(
                    "notebook cell identity/span differs from native diagnostic",
                ));
            }
        }
        for key in [
            "mode",
            "selection",
            "source_classification",
            "source_sha256",
            "notebook_cells",
            "notebook_locations",
            "policy_files",
            "native_settings",
            "original_unchanged",
        ] {
            report.facts.insert(key.into(), value[key].clone());
        }
        if value["native_settings"].as_array().is_none_or(|settings| {
            settings.iter().any(|row| {
                row["exit_code"] != 0 || row["stderr"].as_str().is_none_or(|s| !s.is_empty())
            })
        }) {
            report.complete = false;
        }
        report.limitations.push("Explicit lint, import sorting and formatting share the same Ruff engine and do not provide independent corroboration. Notebook positions refer to captured code-cell IDs; metadata preservation and behavior require separate repair verification.".into());
    }
    let excluded: Vec<_> = requested.difference(&files).copied().collect();
    let scan_exit = value["scan_exit"].as_i64();
    if files.is_empty()
        || !excluded.is_empty()
        || !value["symlinks_not_followed"]
            .as_array()
            .is_some_and(Vec::is_empty)
        || value["inventory_exit"] != 0
        || scan_exit != Some(if diagnostics.is_empty() { 0 } else { 1 })
        || !value["inventory_stderr"]
            .as_str()
            .is_some_and(str::is_empty)
        || !value["native_stderr"].as_str().is_some_and(str::is_empty)
    {
        report.complete = false;
        report.limitations.insert(0, "Ruff coverage is incomplete: inspect excluded inputs, symlinks, empty selection, native diagnostics and process status. An empty finding list cannot clear this scope.".into());
    }
    report
        .facts
        .insert("finding_count".into(), json!(diagnostics.len()));
    report.facts.insert("scan_coverage".into(), json!({"scanned_files":files.len(),"effective_rules":rules.len(),"skipped_files":excluded.len(),"skipped_rules":0,"requested_files":requested,"files":files,"excluded":excluded}));
    report
        .facts
        .insert("diagnostic_details".into(), value["diagnostics"].clone());
    report
        .facts
        .insert("configuration".into(), value["configuration"].clone());
    report
        .facts
        .insert("native_exit".into(), value["scan_exit"].clone());
    Ok(report)
}

#[cfg(test)]
mod selected_native_mode_tests {
    use super::*;
    const FORMAT: &[u8] = include_bytes!("../../tests/fixtures/native-modes/ruff-format.json");
    const NOTEBOOK: &[u8] = include_bytes!("../../tests/fixtures/native-modes/ruff-notebook.json");
    #[test]
    fn native_formatter_and_notebook_locations_stay_distinct() {
        let format = decode(FORMAT).unwrap();
        assert!(format.complete);
        assert_eq!(format.observations[0].rule, "unformatted");
        let notebook = decode(NOTEBOOK).unwrap();
        assert!(notebook.complete);
        assert_eq!(notebook.observations[0].line, None);
        assert_eq!(notebook.facts["notebook_locations"][0]["cell_id"], "calc-a");
        assert_eq!(
            notebook.facts["notebook_locations"][0]["cell_local_location"]["row"],
            1
        );
    }
    #[test]
    fn excluded_and_unsupported_scope_never_clear() {
        assert!(
            !decode(include_bytes!(
                "../../tests/fixtures/native-modes/ruff-excluded.json"
            ))
            .unwrap()
            .complete
        );
        let gap = decode(include_bytes!(
            "../../tests/fixtures/native-modes/ruff-scope-error.json"
        ))
        .unwrap();
        assert!(!gap.complete);
        assert!(gap.observations.is_empty());
        assert!(gap.facts["native_capture_error"]
            .as_str()
            .unwrap()
            .contains("magics"));
    }
    #[test]
    fn notebook_cell_identity_cannot_be_relabelled() {
        let mut value: Value = serde_json::from_slice(NOTEBOOK).unwrap();
        value["notebook_locations"][0]["cell_id"] = json!("wrong-cell");
        assert!(decode(&serde_json::to_vec(&value).unwrap()).is_err());
    }
}
