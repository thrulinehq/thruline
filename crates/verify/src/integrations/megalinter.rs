#[path = "megalinter_controlled.rs"]
mod controlled;
use super::{parse_json, DecodeError, Observation, Report};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
fn bad(message: &str) -> DecodeError {
    DecodeError::Invalid(message.into())
}
fn rows<'a>(v: &'a Value, key: &str) -> Result<&'a Vec<Value>, DecodeError> {
    v[key]
        .as_array()
        .ok_or_else(|| bad(&format!("missing MegaLinter array {key}")))
}
pub fn decode(bytes: &[u8]) -> Result<Report, DecodeError> {
    let v = parse_json(bytes)?;
    if v["schema"] == "thruline.megalinter-capture.v2" {
        return controlled::decode(v);
    }
    if v["schema"] != "thruline.megalinter-capture.v1"
        || v["version"] != "10.1.0"
        || v["source_commit"] != "9949bad031045f366be2467e00e8371a7328a2e2"
    {
        return Err(bad("unrecognized MegaLinter capture version"));
    }
    let linters = rows(&v, "linters")?;
    let ids: BTreeSet<&str> = linters
        .iter()
        .map(|l| {
            l["id"]
                .as_str()
                .ok_or_else(|| bad("missing underlying linter identity"))
        })
        .collect::<Result<_, _>>()?;
    let native = rows(&v["native_report"], "linters")?;
    if ids != BTreeSet::from(["PYTHON_RUFF", "YAML_YAMLLINT"])
        || linters.len() != 2
        || native.len() != 2
    {
        return Err(bad("missing or unexpected underlying analyzer"));
    }
    for linter in linters {
        if !native
            .iter()
            .any(|row| row["name"] == linter["id"] && row == &linter["native"])
        {
            return Err(bad("underlying result differs from native MegaLinter JSON"));
        }
    }
    let mut report = Report {
        format: "megalinter-json-v1".into(),
        observations: vec![],
        facts: BTreeMap::new(),
        complete: v["scan_complete"] == true
            && v["outcome"] == "completed"
            && [json!(0), json!(1)].contains(&v["native_exit_code"])
            && v["native_report"]["return_code"] == v["native_exit_code"]
            && v["native_fix"]["original_unchanged"] == true
            && rows(&v["coverage"], "excluded_eligible_files")?.is_empty()
            && rows(&v["coverage"], "omissions")?.is_empty()
            && linters.iter().all(|row| {
                row["native"]["files_number"]
                    .as_u64()
                    .is_some_and(|n| n > 0)
            }),
        limitations: vec![],
    };
    let mut details = vec![];
    if v["sarif"]["version"] != "2.1.0" {
        return Err(bad("missing native Ruff SARIF"));
    }
    for (run_index, run) in rows(&v["sarif"], "runs")?.iter().enumerate() {
        if run["tool"]["driver"]["name"] != "ruff (MegaLinter PYTHON_RUFF)" {
            return Err(bad("unexpected SARIF underlying analyzer"));
        }
        for (index, row) in rows(run, "results")?.iter().enumerate() {
            let rule = row["ruleId"]
                .as_str()
                .ok_or_else(|| bad("missing Ruff rule"))?;
            let location = &row["locations"][0]["physicalLocation"];
            let path = location["artifactLocation"]["uri"]
                .as_str()
                .ok_or_else(|| bad("missing Ruff SARIF location"))?
                .strip_prefix("file://")
                .ok_or_else(|| bad("unknown Ruff SARIF URI form"))?;
            let pointer = format!("/sarif/runs/{run_index}/results/{index}");
            report.observations.push(Observation {
                rule: rule.into(),
                message: row["message"]["text"]
                    .as_str()
                    .ok_or_else(|| bad("missing Ruff SARIF message"))?
                    .into(),
                path: Some(path.into()),
                line: location["region"]["startLine"].as_u64(),
                pointer: pointer.clone(),
            });
            details.push(json!({"engine":"ruff","version":"0.16.7","rule":rule,"severity":row["level"],"location":location,"pointer":pointer,"independent_of_direct_ruff":false,"native":row}));
        }
    }
    let files = v["source_identity"]["snapshot"]["files"]
        .as_object()
        .ok_or_else(|| bad("missing source snapshot"))?;
    let yaml = linters
        .iter()
        .position(|r| r["id"] == "YAML_YAMLLINT")
        .ok_or_else(|| bad("missing YAML analyzer"))?;
    let mut yaml_count = 0;
    for (log_index, log) in rows(&linters[yaml], "logs")?.iter().enumerate() {
        let text = log["text"]
            .as_str()
            .ok_or_else(|| bad("missing yamllint text"))?;
        let mut path = None;
        for (line_index, line) in text.lines().enumerate() {
            if files.contains_key(line) {
                path = Some(line);
                continue;
            }
            let Some(path) = path else {
                continue;
            };
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 4 {
                continue;
            }
            let Some((line_no, column)) = parts[0].split_once(':') else {
                continue;
            };
            let (Ok(line_no), Ok(column)) = (line_no.parse::<u64>(), column.parse::<u64>()) else {
                continue;
            };
            if line_no == 0 || column == 0 || !["warning", "error"].contains(&parts[1]) {
                return Err(bad("invalid yamllint location/severity"));
            }
            let rule = parts
                .last()
                .and_then(|p| p.strip_prefix('('))
                .and_then(|p| p.strip_suffix(')'))
                .ok_or_else(|| bad("missing yamllint rule"))?;
            let pointer = format!("/linters/{yaml}/logs/{log_index}/text");
            report.observations.push(Observation {
                rule: format!("yamllint:{rule}"),
                message: parts[2..parts.len() - 1].join(" "),
                path: Some(path.into()),
                line: Some(line_no),
                pointer: pointer.clone(),
            });
            details.push(json!({"engine":"yamllint","version":"1.37.1","rule":rule,"severity":parts[1],"path":path,"line":line_no,"column":column,"pointer":pointer,"native_log_line":line_index+1}));
            yaml_count += 1;
        }
    }
    if linters[yaml]["native"]["return_code"] != 0 && yaml_count == 0 {
        report.complete = false;
        report.limitations.push("yamllint reported failure but its native diagnostic lines could not be mapped; inspect the preserved non-SARIF log.".into());
    }
    report
        .facts
        .insert("finding_count".into(), json!(report.observations.len()));
    report
        .facts
        .insert("observation_details".into(), json!(details));
    for key in [
        "linters",
        "native_report",
        "sarif",
        "coverage",
        "source_identity",
        "configuration",
        "runtime",
        "native_fix",
        "advisor",
        "advisor_native_pass",
        "model_provenance",
        "model_provenance_sha256",
        "native_artifacts",
        "artifact_directory",
    ] {
        report.facts.insert(key.into(), v[key].clone());
    }
    for note in rows(&v, "limitations")? {
        report.limitations.push(
            note.as_str()
                .ok_or_else(|| bad("invalid MegaLinter limitation"))?
                .into(),
        );
    }
    if v["advisor"]["status"] == "unavailable" {
        report.limitations.push("The requested optional Advisor did not complete. Deterministic analyzer observations are preserved independently of that advisory limitation.".into());
    }
    if !report.complete {
        report.limitations.push("Both underlying analyzers did not establish complete disclosed scope; no zero-finding success is inferred.".into());
    }
    Ok(report)
}
