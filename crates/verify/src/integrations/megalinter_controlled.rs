use super::super::{DecodeError, Observation, Report};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

fn bad(message: &str) -> DecodeError {
    DecodeError::Invalid(message.into())
}
fn list<'a>(v: &'a Value, key: &str) -> Result<&'a Vec<Value>, DecodeError> {
    v[key]
        .as_array()
        .ok_or_else(|| bad(&format!("missing controlled MegaLinter array {key}")))
}
fn ids(rows: &[Value], key: &str) -> Result<BTreeSet<String>, DecodeError> {
    let values = rows
        .iter()
        .map(|r| {
            r[key]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| bad("native analyzer identity missing"))
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    if values.len() != rows.len() {
        return Err(bad("duplicate native analyzer identity"));
    }
    Ok(values)
}
pub(super) fn decode(v: Value) -> Result<Report, DecodeError> {
    if v["version"] != "10.1.0"
        || v["source_commit"] != "9949bad031045f366be2467e00e8371a7328a2e2"
        || v["selection"]["schema"] != "thruline.megalinter-selection.v2"
    {
        return Err(bad(
            "unrecognized controlled MegaLinter runtime or selection",
        ));
    }
    let requested = list(&v, "selected_linters")?;
    let expected = requested
        .iter()
        .map(|r| {
            r.as_str()
                .map(str::to_owned)
                .ok_or_else(|| bad("requested analyzer identity missing"))
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    let applicability = list(&v, "applicability")?;
    let linters = list(&v, "linters")?;
    if expected.is_empty()
        || expected.len() != requested.len()
        || expected != ids(applicability, "id")?
        || expected != ids(linters, "id")?
        || v["selection"]["linters"] != v["selected_linters"]
    {
        return Err(bad(
            "requested/applicable/native analyzer denominator differs",
        ));
    }
    let events = list(&v, "native_events")?;
    let mut complete = v["scan_complete"] == true
        && v["complete"] == true
        && v["outcome"] == "completed"
        && v["native_fix"]["original_unchanged"] == true
        && v["coverage"]["captured_workspace_unchanged"] == true
        && v["artifact_closure"]["complete"] == true
        && list(&v["artifact_closure"], "gaps")?.is_empty()
        && v["native_report"].is_object()
        && v["native_report"]["return_code"] == v["native_exit_code"]
        && v["namespace"]["State"]["Running"] == false
        && v["namespace"]["State"]["Pid"] == 0
        && v["namespace"]["HostConfig"]["NetworkMode"] == "none"
        && v["namespace"]["HostConfig"]["IpcMode"] == "private"
        && v["namespace"]["HostConfig"]["ReadonlyRootfs"] == true;
    let empty = vec![];
    let native = v["native_report"]["linters"].as_array().unwrap_or(&empty);
    for item in applicability {
        let id = &item["id"];
        let final_row = &item["native_final"];
        let native_row = linters
            .iter()
            .find(|r| r["id"] == *id)
            .ok_or_else(|| bad("requested native row missing"))?;
        if !native_row["native"].is_null()
            && !native
                .iter()
                .any(|r| r["name"] == *id && r == &native_row["native"])
        {
            return Err(bad("derived native row differs from native JSON reporter"));
        }
        if !final_row.is_null()
            && !v["native_final"]["linters"]
                .as_array()
                .is_some_and(|rows| rows.contains(final_row))
        {
            return Err(bad(
                "applicability differs from observed native final state",
            ));
        }
        if !item["native_factory"].is_null()
            && !v["native_inventory"]
                .as_array()
                .is_some_and(|rows| rows.contains(&item["native_factory"]))
        {
            return Err(bad(
                "applicability differs from native descriptor inventory",
            ));
        }
        let completed = events
            .iter()
            .filter(|e| e["kind"] == "native.linter.completed" && e["linter"] == *id)
            .collect::<Vec<_>>();
        if item["native_event_count"].as_u64() != Some(completed.len() as u64) {
            return Err(bad("native linter execution count differs"));
        }
        let timed_out = completed.iter().any(|e| e["return_code"] == 124);
        if timed_out && item["status"] != "timed_out" {
            return Err(bad("native timeout was promoted to another outcome"));
        }
        if [json!("completed_exit_zero"), json!("completed_diagnostics")].contains(&item["status"])
        {
            if timed_out
                || completed.is_empty()
                || final_row["is_active"] != true
                || final_row["files_number"].as_u64().is_none_or(|n| n == 0)
                || ![json!(0), json!(1)].contains(&final_row["return_code"])
            {
                return Err(bad("unperformed linter was promoted to completion"));
            }
            if (item["status"] == "completed_exit_zero") != (final_row["return_code"] == 0) {
                return Err(bad("native exit classification differs"));
            }
        } else {
            complete = false;
        }
    }
    if events.iter().any(|e| {
        e["kind"] == "native.command.failed"
            || e["kind"] == "native.exception"
            || (e["kind"] == "native.command.completed" && e["result"]["status"] != 0)
    }) {
        complete = false;
    }
    let mut report = Report {
        format: "megalinter-json-v2".into(),
        observations: vec![],
        facts: BTreeMap::new(),
        complete,
        limitations: vec![],
    };
    let source_files = v["source_identity"]["snapshot"]["files"]
        .as_object()
        .ok_or_else(|| bad("captured source inventory missing"))?;
    let mut details = vec![];
    if v["sarif"].is_object() {
        if v["sarif"]["version"] != "2.1.0" {
            return Err(bad("unsupported native SARIF version"));
        }
        for (run_i, run) in list(&v["sarif"], "runs")?.iter().enumerate() {
            if run["tool"]["driver"]["name"] != "ruff (MegaLinter PYTHON_RUFF)" {
                return Err(bad("unexpected SARIF producer"));
            }
            for (i, row) in list(run, "results")?.iter().enumerate() {
                let loc = &row["locations"][0]["physicalLocation"];
                let path = loc["artifactLocation"]["uri"]
                    .as_str()
                    .and_then(|x| x.strip_prefix("file://"))
                    .ok_or_else(|| bad("native Ruff location missing"))?;
                if !source_files.contains_key(path) {
                    return Err(bad("native diagnostic is outside captured source"));
                }
                let rule = row["ruleId"]
                    .as_str()
                    .ok_or_else(|| bad("native Ruff rule missing"))?;
                let pointer = format!("/sarif/runs/{run_i}/results/{i}");
                report.observations.push(Observation {
                    rule: rule.into(),
                    message: row["message"]["text"]
                        .as_str()
                        .ok_or_else(|| bad("native Ruff text missing"))?
                        .into(),
                    path: Some(path.into()),
                    line: loc["region"]["startLine"].as_u64(),
                    pointer: pointer.clone(),
                });
                details.push(json!({"engine":"ruff","version":"0.16.7","rule":rule,"severity":row["level"],"location":loc,"pointer":pointer,"independent_of_direct_ruff":false,"native":row}));
            }
        }
    } else if applicability.iter().any(|r| {
        r["id"] == "PYTHON_RUFF"
            && [json!("completed_exit_zero"), json!("completed_diagnostics")].contains(&r["status"])
    }) {
        report.complete = false;
    }
    for (index, linter) in linters.iter().enumerate() {
        let id = linter["id"]
            .as_str()
            .ok_or_else(|| bad("native identity missing"))?;
        if id == "YAML_YAMLLINT" {
            let mut count = 0;
            // MegaLinter's text reporter drops successful stdout, including
            // yamllint warnings returned with exit zero. Preserve the actual
            // native execute_lint_command output instead of assuming no output.
            for (event_index, event) in events.iter().enumerate().filter(|(_, e)| {
                e["kind"] == "native.linter.completed" && e["linter"] == "YAML_YAMLLINT"
            }) {
                let mut path = None;
                for line in event["native_output"]
                    .as_str()
                    .ok_or_else(|| bad("native YAML command output missing"))?
                    .lines()
                {
                    if source_files.contains_key(line) {
                        path = Some(line);
                        continue;
                    }
                    let Some(path) = path else { continue };
                    let parts = line.split_whitespace().collect::<Vec<_>>();
                    if parts.len() < 4 {
                        continue;
                    }
                    let Some((number, column)) = parts[0].split_once(':') else {
                        continue;
                    };
                    let (Ok(number), Ok(column)) = (number.parse::<u64>(), column.parse::<u64>())
                    else {
                        continue;
                    };
                    if number == 0 || column == 0 || !["warning", "error"].contains(&parts[1]) {
                        return Err(bad("invalid YAML native location"));
                    }
                    let rule = parts
                        .last()
                        .and_then(|x| x.strip_prefix('('))
                        .and_then(|x| x.strip_suffix(')'))
                        .ok_or_else(|| bad("native YAML rule missing"))?;
                    let pointer = format!("/native_events/{event_index}/native_output");
                    report.observations.push(Observation {
                        rule: format!("yamllint:{rule}"),
                        message: parts[2..parts.len() - 1].join(" "),
                        path: Some(path.into()),
                        line: Some(number),
                        pointer: pointer.clone(),
                    });
                    details.push(json!({"engine":"yamllint","version":"1.37.1","rule":rule,"severity":parts[1],"path":path,"line":number,"column":column,"pointer":pointer}));
                    count += 1;
                }
            }
            if linter["native"]["return_code"] == 1 && count == 0 {
                report.complete = false;
            }
        } else if id.starts_with("LOCAL_")
            && linter["native"]["return_code"] == 1
            && applicability
                .iter()
                .any(|r| r["id"] == id && r["status"] == "completed_diagnostics")
        {
            report.observations.push(Observation {rule:format!("megalinter-plugin:{id}"),message:format!("Configured native plugin {id} returned diagnostics; inspect its exact native output."),path:None,line:None,pointer:format!("/linters/{index}/native")});
            details.push(json!({"engine":id,"classification":"configured_local_plugin","native":linter["native"],"independent_acceptance":false}));
        }
    }
    report
        .facts
        .insert("finding_count".into(), json!(report.observations.len()));
    report
        .facts
        .insert("observation_details".into(), json!(details));
    for key in [
        "selection",
        "linters",
        "applicability",
        "native_inventory",
        "parallel_observations",
        "command_results",
        "native_events",
        "native_report",
        "native_final",
        "sarif",
        "coverage",
        "source_identity",
        "configuration",
        "runtime",
        "native_fix",
        "native_artifacts",
        "artifact_directory",
        "artifact_closure",
        "namespace",
        "effective_limits",
    ] {
        report.facts.insert(key.into(), v[key].clone());
    }
    report
        .facts
        .insert("megalinter_controlled".into(), json!(true));
    for value in list(&v, "limitations")? {
        report.limitations.push(
            value
                .as_str()
                .ok_or_else(|| bad("invalid scope limitation"))?
                .into(),
        );
    }
    if !report.complete {
        report.limitations.push("Requested native work did not establish complete selected scope. Timeout, missing applicability, failed commands and missing evidence are not clean results.".into());
    }
    Ok(report)
}
